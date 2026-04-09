#!/usr/bin/env python3
"""Fix remaining encoding issues in README.md"""

file_path = r"D:\quasar\README.md"

with open(file_path, 'r', encoding='utf-8') as f:
    text = f.read()

# Fix checkmark emojis
text = text.replace('âœ…', '✅')
text = text.replace(':white_check_mark:', '✅')

with open(file_path, 'w', encoding='utf-8', newline='\n') as f:
    f.write(text)

print("✅ Fixed remaining encoding issues in README.md")
