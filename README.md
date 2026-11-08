# JotPad

JotPad is an open-source, lightweight desktop writing app for Linux, built in Rust. The first milestone focuses on a usable editor and text-focused Microsoft Word (`.docx`) compatibility for school documents.

## Current features

- Create, open, edit, and save UTF-8 plain-text files (`.txt`, `.md`, and other text files)
- Open `.docx` files by extracting paragraph text
- Save `.docx` files that Word and Google Docs can open, with one document paragraph per editor line
- Dark mode toggle, rounded toolbar buttons, and a compact word/character count
- Unsaved-changes warning and `Ctrl+N`, `Ctrl+O`, `Ctrl+S` shortcuts

## DOCX compatibility scope

JotPad currently works with the text content and paragraph breaks of DOCX documents. It does **not** yet preserve or edit Word formatting, page layout, tables, images, headers/footers, comments, or footnotes. Opening and saving an existing formatted school document can therefore lose information. Keep the original file and use a copy when trying DOCX support. The goal is to expand this supported subset and verify it against sample documents from Word and Google Docs before claiming broader compatibility.

## Build on MX Linux

Install the Rust toolchain and common native build dependencies:

```bash
sudo apt update
sudo apt install build-essential pkg-config libgtk-3-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

Then build and run:

```bash
git clone https://github.com/aayanmasud/jotpad.git
cd jotpad
cargo run
```

For an optimized build:

```bash
cargo build --release
./target/release/jotpad
```

## Roadmap

1. Test everyday editing and DOCX text import/export on MX Linux.
2. Add cursor position and reliable document-editing behavior.
3. Design a rich-text document model before implementing formatting controls.
4. Improve DOCX formatting support incrementally and test round-trips with Word and Google Docs.
