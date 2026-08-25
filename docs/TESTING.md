# Devlish Testing Guide

## Quick Test Without Claude API

You can test the executor directly with pre-written Devlish code:

```bash
cd /home/claude/devlish
```

### Test 1: Basic Execution

Create a test script:

```ruby
# test_basic.rb
require_relative 'lib/devlish'

# Load sample document
document = File.read('examples/sample_contract.txt')

# Create a simple Devlish script
devlish_code = <<~CODE
  check "liability clause" do
    require_presence confidence: 0.8
  end

  extract "liability cap" do
    pattern /liability.*cap.*\$?([0-9,]+)/i
    type :currency
    store_as :liability_cap
  end

  extract "contract value" do
    pattern /contract value.*\$?([0-9,]+)/i
    type :currency
    store_as :contract_value
  end

  validate :liability_cap do
    minimum 1_000_000
    flag_if_below severity: :high
  end
CODE

# Execute
result = Devlish.execute(devlish_code, document: document)

puts "Execution: #{result.success? ? 'SUCCESS' : 'FAILED'}"
puts "\nResults:"
puts JSON.pretty_generate(result.to_h)
```

Run it:

```bash
ruby test_basic.rb
```

## Test With Claude API Translation

### Prerequisites

1. Get your Claude API key from: https://console.anthropic.com/
2. Set environment variable:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Test 2: Interactive REPL

Start the REPL:

```bash
./bin/devlish
```

Test commands:

```
devlish> help
devlish> context document_path examples/sample_contract.txt
devlish> Check if the contract contains a liability clause and termination clause
devlish> show
devlish> save test_validation
```

### Test 3: Translation Only

```bash
./bin/devlish translate "Extract the contract value and verify it's above 100000 dollars"
```

### Test 4: Full Workflow

1. Start REPL:
```bash
./bin/devlish
```

2. Set document context:
```
devlish> context document "$(cat examples/sample_contract.txt)"
```

3. Translate English:
```
devlish> Check for liability clause, extract the liability cap amount, and flag if less than 1 million
```

4. Review generated code:
```
devlish> show
```

5. Execute:
```
devlish> execute
```

6. Save for reuse:
```
devlish> save contract_validation
```

## Expected Results

### Sample Output from Test 1

```json
{
  "success": true,
  "operations_count": 8,
  "results": {
    "checks": [
      {
        "target": "liability clause",
        "found": true,
        "confidence": 0.8,
        "passed": true
      }
    ],
    "extractions": [
      {
        "target": "liability cap",
        "value": 500000.0,
        "type": "currency"
      },
      {
        "target": "contract value",
        "value": 250000.0,
        "type": "currency"
      }
    ],
    "validations": [
      {
        "target": "liability_cap",
        "value": 500000.0,
        "passed": false
      }
    ],
    "flags": [
      {
        "target": "liability_cap",
        "reason": "below minimum",
        "severity": "high"
      }
    ]
  }
}
```

## Troubleshooting

### Issue: "Claude API key not configured"

```bash
# Check if set
echo $ANTHROPIC_API_KEY

# Set it
export ANTHROPIC_API_KEY="your-key-here"
```

### Issue: Ruby syntax errors

Make sure you're using Ruby 3.0+:

```bash
ruby --version
```

### Issue: Missing dependencies

```bash
bundle install
```

### Issue: Translation produces invalid code

1. Enable debug mode:
```bash
export DEVLISH_DEBUG=true
```

2. Review the translation
3. Adjust your English to be more specific
4. Manually edit if needed

## Advanced Testing

### Custom Document

Create your own test document:

```bash
cat > examples/my_test.txt << 'EOF'
Your document content here...
EOF
```

Test with it:

```
devlish> context document "$(cat examples/my_test.txt)"
devlish> Your English validation here
devlish> execute
```

### Batch Testing

Create a test suite:

```ruby
# test_suite.rb
require_relative 'lib/devlish'

test_cases = [
  {
    name: "Contract value extraction",
    english: "Extract the contract value",
    document: File.read('examples/sample_contract.txt')
  },
  {
    name: "Liability check",
    english: "Check for liability clause",
    document: File.read('examples/sample_contract.txt')
  }
]

test_cases.each do |test|
  puts "\nTesting: #{test[:name]}"
  code = Devlish.translate(test[:english])
  result = Devlish.execute(code, document: test[:document])
  puts "  Status: #{result.success? ? 'PASS' : 'FAIL'}"
end
```

## Performance Testing

Time the operations:

```ruby
require 'benchmark'

Benchmark.bm do |x|
  x.report("translation:") { Devlish.translate("Check for terms") }
  x.report("validation:") { Devlish.validate(code) }
  x.report("execution:") { Devlish.execute(code, context) }
end
```

## Next Steps

1. Try different English descriptions
2. Test edge cases
3. Build complex multi-step validations
4. Integrate into your workflow
5. Create custom operations

Happy testing! 🚀
