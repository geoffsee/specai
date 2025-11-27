use crate::tools::{Tool, ToolResult};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[cfg(not(target_os = "macos"))]
use extractous::Extractor;

/// Arguments accepted by the file_extract tool
#[derive(Debug, Deserialize)]
struct FileExtractArgs {
    path: String,
    #[serde(default)]
    include_metadata: bool,
    #[serde(default)]
    xml_output: bool,
    #[serde(default)]
    max_chars: Option<i32>,
}

/// Output payload returned by the file_extract tool
#[derive(Debug, Serialize, Deserialize)]
struct FileExtractOutput {
    path: String,
    content: String,
    metadata: Option<HashMap<String, Vec<String>>>,
}

/// Tool that extracts text from files.
/// On macOS: Uses native Vision framework for OCR and PDFKit for PDFs
/// On other platforms: Uses Extractous (Tika-based)
pub struct FileExtractTool;

impl Default for FileExtractTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileExtractTool {
    pub fn new() -> Self {
        Self
    }

    fn normalize_path(&self, input: &str) -> Result<PathBuf> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("file_extract requires a valid path"));
        }
        Ok(PathBuf::from(trimmed))
    }
}

// macOS implementation using native Vision/PDFKit
#[cfg(target_os = "macos")]
mod macos_extract {
    use super::*;
    use std::process::Command;
    use tokio::task;

    /// Swift script that uses Vision framework for OCR and PDFKit for PDF text extraction
    const SWIFT_EXTRACTOR: &str = r#"
import Foundation
import Vision
import PDFKit
import UniformTypeIdentifiers

struct ExtractionResult: Codable {
    let content: String
    let metadata: [String: [String]]?
    let error: String?
}

func extractText(from path: String, includeMetadata: Bool, maxChars: Int?) -> ExtractionResult {
    let url = URL(fileURLWithPath: path)
    let pathExtension = url.pathExtension.lowercased()

    // Determine file type
    var uti: UTType?
    if let typeIdentifier = try? url.resourceValues(forKeys: [.typeIdentifierKey]).typeIdentifier {
        uti = UTType(typeIdentifier)
    }

    // PDF handling
    if pathExtension == "pdf" || uti?.conforms(to: .pdf) == true {
        return extractFromPDF(url: url, includeMetadata: includeMetadata, maxChars: maxChars)
    }

    // Image handling (use Vision OCR)
    let imageExtensions = ["png", "jpg", "jpeg", "tiff", "tif", "gif", "bmp", "heic", "webp"]
    if imageExtensions.contains(pathExtension) || uti?.conforms(to: .image) == true {
        return extractFromImage(url: url, includeMetadata: includeMetadata, maxChars: maxChars)
    }

    // Plain text and other text-based files
    let textExtensions = ["txt", "md", "json", "xml", "html", "htm", "css", "js", "ts", "py", "rs", "go", "java", "c", "cpp", "h", "hpp", "swift", "rb", "php", "yaml", "yml", "toml", "ini", "cfg", "conf", "sh", "bash", "zsh", "csv", "log"]
    if textExtensions.contains(pathExtension) || uti?.conforms(to: .text) == true || uti?.conforms(to: .sourceCode) == true {
        return extractFromText(url: url, includeMetadata: includeMetadata, maxChars: maxChars)
    }

    // Try as text file as fallback
    return extractFromText(url: url, includeMetadata: includeMetadata, maxChars: maxChars)
}

func extractFromPDF(url: URL, includeMetadata: Bool, maxChars: Int?) -> ExtractionResult {
    guard let document = PDFDocument(url: url) else {
        return ExtractionResult(content: "", metadata: nil, error: "Failed to open PDF document")
    }

    var text = ""
    for i in 0..<document.pageCount {
        if let page = document.page(at: i), let pageText = page.string {
            text += pageText
            if i < document.pageCount - 1 {
                text += "\n\n"
            }
        }
    }

    // If PDF has no extractable text, try OCR on each page
    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        text = ocrPDFPages(document: document)
    }

    if let maxChars = maxChars, text.count > maxChars {
        text = String(text.prefix(maxChars))
    }

    var metadata: [String: [String]]? = nil
    if includeMetadata, let attrs = document.documentAttributes {
        var meta: [String: [String]] = [:]
        if let title = attrs[PDFDocumentAttribute.titleAttribute] as? String {
            meta["title"] = [title]
        }
        if let author = attrs[PDFDocumentAttribute.authorAttribute] as? String {
            meta["author"] = [author]
        }
        if let subject = attrs[PDFDocumentAttribute.subjectAttribute] as? String {
            meta["subject"] = [subject]
        }
        if let creator = attrs[PDFDocumentAttribute.creatorAttribute] as? String {
            meta["creator"] = [creator]
        }
        meta["pageCount"] = [String(document.pageCount)]
        metadata = meta.isEmpty ? nil : meta
    }

    return ExtractionResult(content: text, metadata: metadata, error: nil)
}

func ocrPDFPages(document: PDFDocument) -> String {
    var allText = ""
    let semaphore = DispatchSemaphore(value: 0)

    for i in 0..<document.pageCount {
        guard let page = document.page(at: i) else { continue }
        let bounds = page.bounds(for: .mediaBox)

        // Render page to image
        let image = NSImage(size: bounds.size)
        image.lockFocus()
        if let context = NSGraphicsContext.current?.cgContext {
            context.setFillColor(NSColor.white.cgColor)
            context.fill(bounds)
            page.draw(with: .mediaBox, to: context)
        }
        image.unlockFocus()

        guard let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else { continue }

        let request = VNRecognizeTextRequest { request, error in
            defer { semaphore.signal() }
            guard let observations = request.results as? [VNRecognizedTextObservation] else { return }
            let pageText = observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
            if !pageText.isEmpty {
                if !allText.isEmpty { allText += "\n\n" }
                allText += pageText
            }
        }
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true

        let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
        try? handler.perform([request])
        semaphore.wait()
    }

    return allText
}

func extractFromImage(url: URL, includeMetadata: Bool, maxChars: Int?) -> ExtractionResult {
    guard let image = NSImage(contentsOf: url),
          let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        return ExtractionResult(content: "", metadata: nil, error: "Failed to load image")
    }

    var recognizedText = ""
    let semaphore = DispatchSemaphore(value: 0)

    let request = VNRecognizeTextRequest { request, error in
        defer { semaphore.signal() }
        if let error = error {
            return
        }
        guard let observations = request.results as? [VNRecognizedTextObservation] else { return }
        recognizedText = observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
    }
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true

    let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
    do {
        try handler.perform([request])
        semaphore.wait()
    } catch {
        return ExtractionResult(content: "", metadata: nil, error: "OCR failed: \(error.localizedDescription)")
    }

    if let maxChars = maxChars, recognizedText.count > maxChars {
        recognizedText = String(recognizedText.prefix(maxChars))
    }

    var metadata: [String: [String]]? = nil
    if includeMetadata {
        var meta: [String: [String]] = [:]
        meta["width"] = [String(Int(image.size.width))]
        meta["height"] = [String(Int(image.size.height))]
        metadata = meta
    }

    return ExtractionResult(content: recognizedText, metadata: metadata, error: nil)
}

func extractFromText(url: URL, includeMetadata: Bool, maxChars: Int?) -> ExtractionResult {
    do {
        var content = try String(contentsOf: url, encoding: .utf8)
        if let maxChars = maxChars, content.count > maxChars {
            content = String(content.prefix(maxChars))
        }

        var metadata: [String: [String]]? = nil
        if includeMetadata {
            let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
            var meta: [String: [String]] = [:]
            if let size = attrs[.size] as? Int {
                meta["size"] = [String(size)]
            }
            if let modified = attrs[.modificationDate] as? Date {
                meta["modified"] = [ISO8601DateFormatter().string(from: modified)]
            }
            metadata = meta.isEmpty ? nil : meta
        }

        return ExtractionResult(content: content, metadata: metadata, error: nil)
    } catch {
        return ExtractionResult(content: "", metadata: nil, error: "Failed to read file: \(error.localizedDescription)")
    }
}

// Main
let args = CommandLine.arguments
guard args.count >= 2 else {
    let result = ExtractionResult(content: "", metadata: nil, error: "Usage: swift extract.swift <path> [includeMetadata] [maxChars]")
    print(String(data: try! JSONEncoder().encode(result), encoding: .utf8)!)
    exit(1)
}

let path = args[1]
let includeMetadata = args.count > 2 && args[2] == "true"
let maxChars: Int? = args.count > 3 ? Int(args[3]) : nil

let result = extractText(from: path, includeMetadata: includeMetadata, maxChars: maxChars)
let encoder = JSONEncoder()
encoder.outputFormatting = .sortedKeys
if let json = try? encoder.encode(result), let jsonString = String(data: json, encoding: .utf8) {
    print(jsonString)
} else {
    print("{\"content\":\"\",\"error\":\"JSON encoding failed\"}")
}
"#;

    #[derive(Debug, Deserialize)]
    struct SwiftResult {
        content: String,
        metadata: Option<HashMap<String, Vec<String>>>,
        error: Option<String>,
    }

    pub async fn extract_file(
        path: &str,
        include_metadata: bool,
        max_chars: Option<i32>,
    ) -> Result<(String, Option<HashMap<String, Vec<String>>>)> {
        let path = path.to_string();
        let script = SWIFT_EXTRACTOR.to_string();

        task::spawn_blocking(move || {
            // Write Swift script to temp file
            let temp_dir = std::env::temp_dir();
            let script_path = temp_dir.join("spec_ai_extractor.swift");
            fs::write(&script_path, &script).context("Failed to write Swift extractor script")?;

            // Build arguments
            let mut args = vec![script_path.to_string_lossy().to_string(), path.clone()];
            args.push(include_metadata.to_string());
            if let Some(max) = max_chars {
                args.push(max.to_string());
            }

            // Execute Swift script
            let output = Command::new("swift")
                .args(&args)
                .output()
                .context("Failed to execute Swift script. Ensure Xcode/Swift is installed.")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!("Swift extraction failed: {}", stderr));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let result: SwiftResult =
                serde_json::from_str(&stdout).context("Failed to parse Swift output")?;

            if let Some(error) = result.error {
                return Err(anyhow!("Extraction error: {}", error));
            }

            Ok((result.content, result.metadata))
        })
        .await
        .context("Task join error")?
    }
}

#[async_trait]
impl Tool for FileExtractTool {
    fn name(&self) -> &str {
        "file_extract"
    }

    fn description(&self) -> &str {
        "Extracts text and metadata from files regardless of format (PDF, Office, HTML, images with OCR, etc.)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative or absolute path to the file that should be extracted"
                },
                "include_metadata": {
                    "type": "boolean",
                    "description": "Include metadata from the file",
                    "default": false
                },
                "xml_output": {
                    "type": "boolean",
                    "description": "Request XML formatted result instead of plain text (non-macOS only)",
                    "default": false
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Limit the number of characters returned (must be > 0 if provided)",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let args: FileExtractArgs =
            serde_json::from_value(args).context("Failed to parse file_extract arguments")?;

        let path = self.normalize_path(&args.path)?;
        let metadata =
            fs::metadata(&path).with_context(|| format!("File not found: {}", path.display()))?;

        if !metadata.is_file() {
            return Ok(ToolResult::failure(format!(
                "{} is not a regular file",
                path.display()
            )));
        }

        if let Some(max_chars) = args.max_chars {
            if max_chars <= 0 {
                return Ok(ToolResult::failure(
                    "max_chars must be greater than zero".to_string(),
                ));
            }
        }

        let display_path = path.to_string_lossy().into_owned();

        // Platform-specific extraction
        #[cfg(target_os = "macos")]
        let (content, extracted_metadata) = {
            macos_extract::extract_file(&display_path, args.include_metadata, args.max_chars)
                .await
                .map_err(|e| anyhow!("macOS extraction failed: {}", e))?
        };

        #[cfg(not(target_os = "macos"))]
        let (content, extracted_metadata) = {
            let mut extractor = Extractor::new();
            if let Some(max_chars) = args.max_chars {
                extractor = extractor.set_extract_string_max_length(max_chars);
            }
            if args.xml_output {
                extractor = extractor.set_xml_output(true);
            }
            extractor
                .extract_file_to_string(&display_path)
                .map_err(|err| anyhow!("Failed to extract {}: {}", display_path, err))?
        };

        let metadata = if args.include_metadata {
            extracted_metadata
        } else {
            None
        };

        let output = FileExtractOutput {
            path: display_path,
            content,
            metadata,
        };

        Ok(ToolResult::success(
            serde_json::to_string(&output).context("Failed to serialize file_extract output")?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn name_and_description() {
        let tool = FileExtractTool::new();
        assert_eq!(tool.name(), "file_extract");
        assert!(tool.description().contains("Extracts text"));
    }

    #[tokio::test]
    async fn parameters_require_path() {
        let tool = FileExtractTool::new();
        let params = tool.parameters();
        let required = params["required"].as_array().unwrap();
        assert!(required.iter().any(|value| value == "path"));
    }

    #[tokio::test]
    async fn invalid_max_chars_returns_failure() {
        let tool = FileExtractTool::new();
        let tmp = NamedTempFile::new().unwrap();
        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy(),
            "max_chars": 0
        });

        let result = tool.execute(args).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "max_chars must be greater than zero");
    }

    #[tokio::test]
    async fn extract_plain_text_file() {
        let tool = FileExtractTool::new();
        let tmp = NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "Hello, World!").unwrap();

        let args = serde_json::json!({
            "path": tmp.path().to_string_lossy()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.success);
        let output: FileExtractOutput = serde_json::from_str(&result.output).unwrap();
        assert!(output.content.contains("Hello, World!"));
    }
}
