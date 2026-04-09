#!/usr/bin/env python3
"""Fix encoding issues in README.md"""

file_path = r"D:\quasar\README.md"

# Read the file as bytes first
with open(file_path, 'rb') as f:
    content = f.read()

# Decode as UTF-8
text = content.decode('utf-8')

# Replace broken UTF-8 sequences
replacements = {
    '\u00e2\u20ac\u201d': '\u2014',  # â€" → — (em dash)
    '\u00e2\u20ac\u201c': '\u2014',  # â€" → — (em dash variant)
    '\u00e2\u20ac\u201d': '\u2013',  # â€" → – (en dash)
    '\u00e2\u2020\u201d': '\u2194',  # â†" → ↔ (left-right arrow)
    '\u00c3\u2014': '\u00d7',        # Ã— → × (multiplication)
    '\u00f0\u0178\u00a6\u20ac': '\U0001F980',  # ðŸ¦€ → 🦀 (crab)
    '\u00f0\u0178\u0161\u20ac': '\U0001F680',  # ðŸš€ → 🚀 (rocket)
    '\u00e2\u20ac\u201c3': '\u2013',  # â€"3 → –3 (en dash with number)
}

for old, new in replacements.items():
    text = text.replace(old, new)

# Write back as UTF-8
with open(file_path, 'w', encoding='utf-8', newline='\n') as f:
    f.write(text)

print("✅ Fixed encoding issues in README.md")
