<html>
    <center>
        <img src="./assets/all.png" width=15%></img>
        <h3>Type-Safe configuration file</h3>
    </center>
</html>

![CI](https://github.com/liy77/lson/workflows/CI/badge.svg)
![Release](https://github.com/liy77/lson/workflows/Release/badge.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

# LSON - Type-Safe Configuration Parser

LSON is a powerful type-safe configuration file parser that supports the KSON format with automatic builds for multiple platforms.

## 🚀 Installation

### Pre-built Binaries
Download the latest release for your platform from the [Releases page](https://github.com/liy77/lson/releases):

- **Windows**: `lson-windows-x86_64.exe`
- **Linux**: `lson-linux-x86_64`
- **macOS (Intel)**: `lson-macos-x86_64`
- **macOS (Apple Silicon)**: `lson-macos-arm64`

### From Source
```bash
git clone https://github.com/liy77/lson.git
cd lson
cargo build --release
```

## 📖 Understanding KSON
```kson
# config.kson

# Get var USER_TOKEN and USER_KEY in env
@env(USER_TOKEN)
@env(USER_KEY)

username = "Liy"
token = USER_TOKEN
public_key = USER_KEY

# Nested sections
$dependencies
    teste = 1
    $teste2
        git = "https://github.com/example"
        branch = "main"
    outro_campo = "valor"
```

## 📋 Understanding KModel
```kmodel
# config.kmodel

username: String
token: String
public_key: String?

dependencies: {
    teste: Integer
    teste2: {
        git: String
        branch: String
    }
    outro_campo: String
}
```

## 🛠 Usage

### Compile KSON to LSON
```bash
lson compile -f config.kson
```

### Compile KSON to JSON
```bash
lson compile -t json -f config.kson
```

### Parse LSON file
```bash
lson parse config.lson
```

### Using with KModel validation
```bash
lson compile -f config.kson --kmodel config.kmodel -t json
```

### Raw mode (for programmatic use)
```bash
lson raw compile -f config.kson -t json
```

## 🏗 Development

### Building
```bash
cargo build
```

### Testing
```bash
cargo test
```

### Linting
```bash
cargo clippy
cargo fmt
```

## 🤖 Automated Builds

This project uses GitHub Actions for automated building and releasing:

- **CI Pipeline**: Runs on every push/PR, tests across Windows, Linux, and macOS
- **Development Builds**: Creates artifacts for every commit to main branch (30-day retention)
- **Release Pipeline**: Triggered by version tags (e.g., `v1.0.0`), creates GitHub releases with binaries

### Creating a Release
1. Update version in `Cargo.toml`
2. Create and push a tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
3. GitHub Actions will automatically build and create a release

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🤝 Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 🐛 Issues

If you encounter any issues or have suggestions, please [open an issue](https://github.com/liy77/lson/issues).