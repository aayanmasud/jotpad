# JotPad

A lightweight, desktop-focused text editor for Linux, designed to feel minimal, fast, and calm.

## Design direction

- Rounded buttons and subtle hover states
- Square-edged app frames for a clean, structured look
- Gentle transitions and focus cues while typing
- A small, uncluttered status bar
- Built for MX Linux and other Linux desktops with a native GTK-style feel

## Features

- New, open, save, and save-as actions
- Plain text editing with a monospace writing surface
- Unsaved-changes warning before replacing the current document
- Dark mode toggle
- Lightweight desktop app built with `eframe` and `egui`

## Requirements

- Linux
- Rust toolchain

## Run from source

```bash
git clone https://github.com/aayanmasud/jotpad.git
cd jotpad
cargo run
```

## Build release

```bash
cargo build --release
./target/release/jotpad
```
