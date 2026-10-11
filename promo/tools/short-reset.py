"""Adds the short "Use reset" label to every language in src/data.ts."""
import re

SHORT = {
    "en": "Use", "ru": "Использовать", "de": "Nutzen", "fr": "Utiliser",
    "es": "Usar", "pt-BR": "Usar", "ja": "使用", "zh-CN": "使用",
}
path = "src/data.ts"
s = open(path, encoding="utf-8").read()
s = s.replace("  useReset: string;\n", "  useReset: string;\n  /// Fallback when the full label does not fit (use-reset-short).\n  useResetShort: string;\n", 1)
# en is a standalone const; the others are inline in STRINGS.
s = s.replace('  useReset: "Use reset",\n', '  useReset: "Use reset",\n  useResetShort: "Use",\n', 1)
for lang, short in SHORT.items():
    if lang == "en":
        continue
    key = f'"{lang}"' if "-" in lang else lang
    start = s.index(f"  {key}: {{")
    m = re.compile(r'(      useReset: "[^"]*",\n)').search(s, start)
    s = s[: m.end()] + f'      useResetShort: "{short}",\n' + s[m.end():]
open(path, "w", encoding="utf-8").write(s)
