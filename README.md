# MineOS - Open Source Mining Operating System

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)](https://www.rust-lang.org/)

MineOS is a modern, efficient cryptocurrency mining operating system built from scratch in Rust. This repository contains the open-source components that form the foundation of the MineOS ecosystem.

## 🚀 Features

- **High Performance**: Written in Rust for maximum efficiency and safety
- **Multi-Algorithm Support**: SHA-256, Ethash, KawPow, Octopus, and more
- **Hardware Auto-Detection**: Automatic GPU/ASIC detection and configuration
- **Stratum Protocol**: Full Stratum and Stratum V2 support
- **Basic Monitoring**: Real-time hashrate and temperature monitoring
- **CLI Interface**: Powerful command-line tools for management

## 📦 Components

| Component | Description | Status |
|-----------|-------------|--------|
| `mineos-core` | Core mining engine and scheduler | 🚧 In Development |
| `mineos-stratum` | Stratum protocol implementation | 🚧 In Development |
| `mineos-hardware` | Hardware detection and management | 🚧 In Development |
| `mineos-hash` | Mining algorithm implementations | 🚧 In Development |
| `mineos-monitor-basic` | Basic monitoring and metrics | 🚧 In Development |
| `mineos-cli` | Command-line interface | 🚧 In Development |

## 🛠️ Installation

### Prerequisites

- Rust 1.70 or higher
- CUDA Toolkit 12.0+ (for NVIDIA GPUs)
- ROCm 5.0+ (for AMD GPUs)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/mineosdev/mineos.git
cd mineos

# Build all components
cargo build --release

# Run tests
cargo test --all

# Install CLI tool
cargo install --path mineos-cli
```

## 🚀 Quick Start

```bash
# Detect hardware
mineos hardware detect

# Start mining Bitcoin
mineos start --algo sha256 --pool stratum+tcp://pool.example.com:3333 --wallet YOUR_WALLET

# Monitor status
mineos status

# View logs
mineos logs --follow
```

## 📊 Performance

MineOS achieves industry-leading performance through:
- Zero-copy memory management
- Lock-free concurrent data structures
- Optimized GPU kernels
- Efficient work distribution

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
# Fork and clone the repository
git clone https://github.com/YOUR_USERNAME/mineos.git

# Create a feature branch
git checkout -b feature/amazing-feature

# Make your changes and test
cargo test --all

# Submit a pull request
```

## 📝 License

This project is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

You may choose which license you prefer.

## 🏢 Commercial Support

For enterprise features, advanced monitoring, AI optimization, and professional support, check out [MineOS SaaS](https://mineos.io).

### Pricing Tiers

| Tier | Price | Features |
|------|-------|----------|
| **Free** | $0 | Up to 2 rigs, basic features |
| **Pro** | $25/month | Up to 50 rigs, profit switching, analytics |
| **Business** | $299/month | Up to 500 rigs, API access, AI optimization |
| **Enterprise** | Custom | Unlimited rigs, compliance, dedicated support |

## 🗺️ Roadmap

- [x] Core architecture design
- [ ] Basic mining implementation
- [ ] Stratum protocol support
- [ ] GPU support (NVIDIA/AMD)
- [ ] Web dashboard (Pro tier)
- [ ] AI optimization (Business tier)
- [ ] Kubernetes operator

## 📬 Contact

- GitHub Issues: [github.com/mineosdev/mineos/issues](https://github.com/mineosdev/mineos/issues)
- Email: support@mineos.io
- Discord: [discord.gg/mineos](https://discord.gg/mineos)

## 🙏 Acknowledgments

Built with ❤️ by the MineOS team and contributors worldwide.