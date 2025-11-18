# Setup

## Extractous Native Build Setup Guide

This guide walks through the steps to configure GraalVM 23+ and Tesseract for the extractous native build used by the `file_extract` tool.

### Prerequisites

- macOS with Homebrew installed
- Java/GraalVM installation capabilities
- Terminal access

### Step 1: Verify GraalVM Installation

Check if GraalVM is already installed:

```bash
java -version
```

Expected output should show GraalVM CE 23 or higher:
```
OpenJDK Runtime Environment GraalVM CE 24+36.1 (build 24+36-jvmci-b01)
```

#### If GraalVM is Not Installed

Install GraalVM using Homebrew:

```bash
brew install --cask graalvm/tap/graalvm-ce-java24
```

Or use SDKMAN:

```bash
sdk install java 24-graal
```

## Step 2: Locate GraalVM Home Directory

Find the GraalVM installation path:

```bash
/usr/libexec/java_home -v 24
```

Example output:
```
/Users/geoffsee/Library/Java/JavaVirtualMachines/graalvm-ce-24.0.0/Contents/Home
```

## Step 3: Set GRAALVM_HOME Environment Variable

#### For Current Session

```bash
export GRAALVM_HOME="/Users/geoffsee/Library/Java/JavaVirtualMachines/graalvm-ce-24.0.0/Contents/Home"
```

#### For Persistence (zsh)

Add to `~/.zshrc`:

```bash
echo 'export GRAALVM_HOME="/Users/geoffsee/Library/Java/JavaVirtualMachines/graalvm-ce-24.0.0/Contents/Home"' >> ~/.zshrc
```

#### For Persistence (bash)

Add to `~/.bash_profile` or `~/.bashrc`:

```bash
echo 'export GRAALVM_HOME="/Users/geoffsee/Library/Java/JavaVirtualMachines/graalvm-ce-24.0.0/Contents/Home"' >> ~/.bash_profile
```

#### Verify

```bash
echo $GRAALVM_HOME
```

### Step 4: Install Tesseract OCR

Install Tesseract using Homebrew:

```bash
brew install tesseract
```

#### Verify Installation

```bash
tesseract --version
```

Expected output:
```
tesseract 5.5.1
```

#### Check Available Language Packs

```bash
tesseract --list-langs
```

Default installation includes:
- `eng` (English)
- `osd` (Orientation and script detection)

#### Install Additional Language Packs (Optional)

If you need additional languages:

```bash
brew install tesseract-lang
```

### Step 5: Build and Test

Clean and rebuild the project with tests:

```bash
cargo clean && cargo test file_extract
```

Expected output should show all tests passing:
```
running 3 tests
test tools::builtin::file_extract::tests::name_and_description ... ok
test tools::builtin::file_extract::tests::parameters_require_path ... ok
test tools::builtin::file_extract::tests::invalid_max_chars_returns_failure ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

### Troubleshooting

#### GRAALVM_HOME Not Found Error

If you see errors about missing GRAALVM_HOME:

1. Verify the environment variable is set:
   ```bash
   echo $GRAALVM_HOME
   ```

2. If empty, ensure you've sourced your shell configuration:
   ```bash
   source ~/.zshrc  # or ~/.bash_profile
   ```

3. Restart your terminal session

#### Tesseract Not Found Error

If Tesseract is not found:

1. Check if it's installed:
   ```bash
   which tesseract
   ```

2. If not found, install it:
   ```bash
   brew install tesseract
   ```

3. Verify it's in your PATH:
   ```bash
   echo $PATH | grep homebrew
   ```

#### Native Build Fails

If the native build fails during compilation:

1. Ensure GRAALVM_HOME is set in the same terminal session where you run cargo
2. Try a full clean build:
   ```bash
   cargo clean
   cargo build --release
   ```

### Additional Resources

- [GraalVM Official Documentation](https://www.graalvm.org/latest/docs/)
- [Tesseract OCR Documentation](https://github.com/tesseract-ocr/tesseract)
- [Extractous Library](https://github.com/yobix-ai/extractous)

### Summary

Once configured, the `file_extract` tool can extract text content from various document formats including:
- PDF files
- Microsoft Office documents (Word, Excel, PowerPoint)
- Images with OCR (via Tesseract)
- HTML and other text formats
