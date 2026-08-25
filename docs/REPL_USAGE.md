# Devlish REPL - Interactive Prompt Tool

The Devlish REPL now includes a powerful `prompt` command that lets you ask Claude how to convert examples into Devlish programs.

## Starting the REPL

```bash
./bin/devlish repl
```

## Using the Prompt Command

The `prompt` command sends your question to Claude along with comprehensive Devlish language documentation, allowing you to get help converting real-world examples into Devlish code.

### Basic Usage

```
devlish> prompt How do I calculate quarterly estimated taxes?
```

### Example Sessions

#### Example 1: IRS Tax Calculations

```
devlish> prompt Convert these 2025 standard deduction amounts into a Devlish calculator: Single: $15,000, Married Filing Jointly: $30,000, Head of Household: $22,500
```

Claude will respond with something like:

```devlish
Tax's Standard Deduction Calculator:

calculate standard deduction using filing status:
  standard deduction is the IRS standard deduction amount for 2025

  standard deduction equals 15000 if filing status equals single
  standard deduction equals 30000 if filing status equals married filing jointly
  standard deduction equals 22500 if filing status equals head of household

  respond with standard deduction
```

You can then:
1. Review the code
2. Save it when prompted (y/n)
3. Enter a filename (e.g., `examples/tax/standard_deduction`)
4. Run it with the `parse` command

#### Example 2: Business Logic

```
devlish> prompt I need to calculate employee bonuses. Base bonus is $1000. Add $500 if tenure > 5 years. Add $300 if performance rating is "excellent". Show me how.
```

#### Example 3: Financial Calculations

```
devlish> prompt How would I calculate compound interest with monthly contributions in Devlish?
```

### Workflow

1. **Ask your question**
   ```
   devlish> prompt <your question>
   ```

2. **Claude responds** with:
   - Explanation of the approach
   - Complete Devlish code example
   - Comments explaining the logic

3. **Save the code** (optional)
   - Answer `y` when prompted
   - Enter a filename
   - Code is saved as `.dvl` file

4. **Test the code**
   ```
   devlish> parse examples/your_file.dvl
   ```

## What Makes This Powerful

The `prompt` command provides Claude with:

- **Complete Devlish syntax reference**
- **Multi-word variable support**
- **Conditional assignment patterns**
- **Method parameter syntax**
- **Complete working examples**

This means Claude can generate accurate, idiomatic Devlish code that follows all the latest conventions.

## Tips for Good Prompts

### ✅ Good Prompts

- **Specific examples**: "Convert these IRS 401k limits: $23,500 standard, $7,500 catch-up for 50+"
- **Real-world scenarios**: "Calculate sales commission: 5% base, 7% if monthly sales > $50,000"
- **With context**: "HSA contribution limits: $4,300 self, $8,550 family, +$1,000 at age 55"

### ❌ Less Effective Prompts

- Too vague: "How do I do calculations?"
- Too broad: "Show me everything about Devlish"
- Missing details: "Calculate taxes" (which taxes? what rules?)

## Available REPL Commands

```
help                          - Show this help message
prompt <question>             - Ask Claude how to convert examples to Devlish
translate <text>              - Translate English to Devlish
validate <code>               - Validate Devlish code
execute <code>                - Execute Devlish code
save <filename>               - Save last translated code to file
load <filename>               - Load and execute a Devlish file
context <key> <value>         - Set context variable
show                          - Show last translated code
clear                         - Clear screen and history
exit                          - Exit the REPL
```

## Environment Setup

Make sure your `.env` file contains:

```bash
ANTHROPIC_API_KEY=your_api_key_here
```

## Complete Example Session

```
$ ./bin/devlish repl

╔═══════════════════════════════════════════════════╗
║                                                   ║
║   DEVLISH - Build software using English          ║
║   Version 0.1.0                                   ║
║                                                   ║
╚═══════════════════════════════════════════════════╝

Available commands:
  help                          - Show this help message
  prompt <question>             - Ask Claude how to convert examples to Devlish
  ...

Type 'exit' to quit

devlish> prompt How do I calculate social security tax? The rate is 6.2% on wages up to $168,600

🤖 Asking Claude for help...

============================================================
Here's how you would calculate Social Security tax in Devlish:

```devlish
Payroll's Social Security Calculator:

calculate social security tax using gross wages:
  ss tax is Social Security tax amount
  ss rate is Social Security tax rate of 6.2%
  wage base limit is maximum taxable wages for Social Security
  taxable wages is amount subject to Social Security tax

  ss rate equals 0.062
  wage base limit equals 168600

  taxable wages equals gross wages if gross wages <= wage base limit
  taxable wages equals wage base limit if gross wages > wage base limit

  ss tax equals taxable wages * ss rate

  respond with ss tax
```

This calculator:
1. Defines the Social Security tax rate (6.2% = 0.062)
2. Sets the wage base limit ($168,600 for 2025)
3. Determines taxable wages (capped at the limit)
4. Calculates the tax amount

The key feature is the conditional assignment - if wages exceed
the limit, only the limit amount is taxed.
============================================================

Would you like to save this code? (y/n)
y
Enter filename:
examples/payroll/social_security
✓ Saved to examples/payroll/social_security.dvl

devlish> parse examples/payroll/social_security.dvl
...
```

## Next Steps

After generating code with `prompt`:

1. **Parse it** - See the Ruby output: `parse filename.dvl`
2. **Refine it** - Ask follow-up questions
3. **Combine it** - Use inheritance with `based on`
4. **Share it** - Build a library of reusable calculators

Happy coding with Devlish! 🎉
