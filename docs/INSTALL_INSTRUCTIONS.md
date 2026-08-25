# Devlish Installation Instructions

## 📦 What You Have

- `devlish.tar.gz` - Complete Devlish project archive (75 KB)

## 🚀 Installation Steps

### 1. Extract the Archive

Download `devlish.tar.gz` and extract it to your desired location:

```bash
# Navigate to where you want to install
cd /Users/andrew/Dropbox/Cloudze/code/clients/

# Extract the archive
tar -xzf ~/Downloads/devlish.tar.gz

# Navigate into the project
cd devlish
```

### 2. Verify Installation

```bash
# Check that all files are present
ls -la

# You should see:
# - README.md, QUICKSTART.md, DOCUMENTATION.md, etc.
# - bin/devlish (executable)
# - lib/ directory with Ruby code
# - examples/ with sample scripts
```

### 3. Make CLI Executable (if needed)

```bash
chmod +x bin/devlish
```

### 4. Test Without API (Immediate)

```bash
ruby test_basic.rb
```

This will run a complete validation test without requiring any API keys!

### 5. Get Claude API Key (Optional but Recommended)

1. Visit: https://console.anthropic.com/
2. Sign up or log in
3. Generate an API key
4. Set environment variable:

```bash
export ANTHROPIC_API_KEY="sk-ant-your-key-here"
```

### 6. Start Using Devlish

```bash
# Interactive REPL
./bin/devlish

# Translate English to Devlish
./bin/devlish translate "Check if document contains terms"

# Run a script
./bin/devlish run examples/basic_validation.devlish

# Get help
./bin/devlish help
```

## 📚 Next Steps

1. Read `QUICKSTART.md` for a 5-minute tutorial
2. Review `DOCUMENTATION.md` for complete language reference
3. Check out the `examples/` directory for sample scripts
4. Run `ruby test_basic.rb` to see it in action

## 🆘 Troubleshooting

### Ruby Not Found

```bash
# macOS - Install with Homebrew
brew install ruby

# Or use rbenv
brew install rbenv
rbenv install 3.2.0
```

### Permission Denied

```bash
chmod +x bin/devlish
```

### API Key Issues

```bash
# Check if set
echo $ANTHROPIC_API_KEY

# Set temporarily
export ANTHROPIC_API_KEY="sk-ant-..."

# Set permanently (add to ~/.zshrc or ~/.bash_profile)
echo 'export ANTHROPIC_API_KEY="sk-ant-..."' >> ~/.zshrc
```

## 📁 Project Structure

```
devlish/
├── README.md              # Project overview
├── QUICKSTART.md          # Quick tutorial
├── DOCUMENTATION.md       # Complete reference
├── TESTING.md            # Testing guide
├── bin/devlish           # Main executable
├── lib/                  # Ruby source code
├── examples/             # Example scripts
└── test_basic.rb         # Test script
```

## ✅ Verify Installation

Run this to ensure everything works:

```bash
cd /Users/andrew/Dropbox/Cloudze/code/clients/devlish
ruby test_basic.rb
```

You should see validation results displayed!

## 🎉 Success!

You're ready to use Devlish! Start with:

```bash
./bin/devlish
```

For help: Read QUICKSTART.md or type `help` in the REPL.
