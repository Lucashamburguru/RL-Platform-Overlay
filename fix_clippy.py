import re

with open("src/ui/mmr_panel.rs", "r") as f:
    content = f.read()

content = content.replace("trimmed.as_bytes().len() >= 7", "trimmed.len() >= 7")

with open("src/ui/mmr_panel.rs", "w") as f:
    f.write(content)
