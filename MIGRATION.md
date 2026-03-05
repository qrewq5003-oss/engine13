# ENGINE13 Migration Guide: Tauri v1 → v2 + Rust PATH Fix

## Problem
System uses rustc 1.76.0 instead of rustc 1.93.1 from ~/.cargo/bin

## Solution 1: Add PATH to package.json scripts

```json
{
  "scripts": {
    "tauri": "export PATH=/home/deck/.cargo/bin:$PATH && tauri"
  }
}
```

## Solution 2: Add .cargo/config.toml in src-tauri

```toml
[build]
rustc = "/home/deck/.cargo/bin/rustc"
cargo = "/home/deck/.cargo/bin/cargo"
```

## Solution 3: Source cargo env before running

```bash
source ~/.cargo/env
npm run tauri dev
```

## Tauri v2 Migration

### 1. Update src-tauri/Cargo.toml
```toml
[dependencies]
tauri = { version = "2", features = [] }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

### 2. Update package.json
```json
{
  "@tauri-apps/api": "^2",
  "@tauri-apps/cli": "^2"
}
```

### 3. Update main.rs for Tauri v2 syntax
```rust
// Tauri v2 uses different plugin system
#[cfg_attr(mobile, tauri::mobile_entry_point)]
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![...])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 4. Update tauri.conf.json for v2 format
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "identifier": "com.engine13.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420"
  }
}
```
