#!/usr/bin/env python3

with open("D:/quasar/crates/quasar-engine/src/runner.rs", "r", encoding="utf-8") as f:
    content = f.read()

# Fix editor and editor_renderer creation
content = content.replace(
    "        let editor = Editor::new();\n        let editor_renderer =\n            EditorRenderer::new(&window, &renderer.device, renderer.config.format);",
    '        #[cfg(feature = "editor")]\n        let editor = Editor::new();\n        #[cfg(feature = "editor")]\n        let editor_renderer =\n            EditorRenderer::new(&window, &renderer.device, renderer.config.format);',
)

with open("D:/quasar/crates/quasar-engine/src/runner.rs", "w", encoding="utf-8") as f:
    f.write(content)

print("File updated successfully")
