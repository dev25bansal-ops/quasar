import re

with open("Cargo.toml", "r", encoding="utf-8") as f:
    content = f.read()

# Add quasar-templates to members
content = content.replace(
    '"crates/quasar-ai",', '"crates/quasar-ai",\n    "crates/quasar-templates",'
)

# Add quasar-templates to workspace dependencies
content = content.replace(
    'quasar-ai = { path = "crates/quasar-ai" }',
    'quasar-ai = { path = "crates/quasar-ai" }\nquasar-templates = { path = "crates/quasar-templates" }',
)

with open("Cargo.toml", "w", encoding="utf-8") as f:
    f.write(content)

print("Done!")
