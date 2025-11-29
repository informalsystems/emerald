# Emerald Documentation

This documentation is built using [mdBook](https://rust-lang.github.io/mdBook/), a utility to create modern online books from Markdown files.

## Prerequisites

To build and view the documentation locally, you need to install mdBook:

### Installation

**Using Cargo (Rust package manager):**
```bash
cargo install mdbook
```

**On macOS using Homebrew:**
```bash
brew install mdbook
```

**On Linux using pre-built binaries:**
```bash
curl -sSL https://github.com/rust-lang/mdBook/releases/download/v0.4.40/mdbook-v0.4.40-x86_64-unknown-linux-gnu.tar.gz | tar -xz
sudo mv mdbook /usr/local/bin/
```

Verify the installation:
```bash
mdbook --version
```

## Building the Documentation

### Serve Locally (with auto-reload)

To view the documentation locally with automatic reloading on changes:

```bash
cd docs/operational-docs
mdbook serve --open
```

This will:
- Build the documentation
- Start a local web server at `http://localhost:3000`
- Open your browser automatically
- Watch for file changes and rebuild automatically

### Build Static HTML

To build the documentation as static HTML files:

```bash
cd docs/operational-docs
mdbook build
```

The generated HTML will be in the `book/` directory.

### Using Docker

If you prefer to use Docker instead of installing mdBook locally:

**Build the Docker image:**
```bash
cd docs/operational-docs
docker build -t emerald-docs .
```

**Run the container:**
```bash
docker run -p 3000:3000 emerald-docs
```

The documentation will be available at `http://localhost:3000`

**Run with live reload (mount source directory):**
```bash
docker run -p 3000:3000 -v $(pwd)/src:/docs/src emerald-docs
```

This mounts your local `src` directory into the container, allowing you to edit files and see changes reflected immediately.

## Documentation Structure

The documentation follows this structure:

```
docs/operational-docs/
├── book.toml              # mdBook configuration
├── src/                   # Source markdown files
│   ├── SUMMARY.md         # Table of contents (defines navigation)
│   ├── introduction.md    # Main introduction page
│   ├── local-testnet.md   # Running a local testnet guide
│   ├── production-network.md  # Production network setup guide
│   ├── config-examples.md # Configuration examples reference
│   ├── images/            # Image assets
│   └── config-examples/   # Configuration file examples
└── book/                  # Generated HTML output (gitignored)
```

## Adding New Content

### Adding a New Page

1. Create a new Markdown file in the `src/` directory
2. Add the page to `src/SUMMARY.md` to include it in the navigation

**Example:**
```markdown
# Summary

[Introduction](./introduction.md)

- [My New Page](./my-new-page.md)
```

### Adding Images

1. Place image files in `src/images/`
2. Reference them in Markdown using relative paths:
```markdown
![Description](images/my-image.png)
```

### Adding Configuration Examples

1. Place configuration files in `src/config-examples/`
2. Reference them in Markdown:
```markdown
See [config.toml](config-examples/config.toml) for an example.
```

## Markdown Features

mdBook supports standard Markdown plus some additional features:

### Code Blocks with Syntax Highlighting

\`\`\`rust
fn main() {
    println!("Hello, world!");
}
\`\`\`

### Admonitions (Info Boxes)

Use blockquotes with special markers:

```markdown
> **Note:** This is an informational note.

> **Warning:** This is a warning message.

> **Important:** This is an important message.
```

### Links

- Internal links: `[Link Text](./other-page.md)`
- External links: `[Link Text](https://example.com)`
- Anchor links: `[Link Text](./page.md#section)`

## Configuration

The `book.toml` file contains the mdBook configuration:

- **Book metadata**: Title, authors, language
- **Build settings**: Output directory, source directory
- **HTML output**: Theme, search, playground settings
- **Preprocessors**: Link processing, etc.

## Deployment

To deploy the documentation:

1. Build the static HTML:
   ```bash
   mdbook build
   ```

2. Deploy the contents of the `book/` directory to your web server or hosting platform

Common deployment targets:
- GitHub Pages
- Netlify
- Any static site hosting service

## Additional Resources

- [mdBook Documentation](https://rust-lang.github.io/mdBook/)
- [mdBook GitHub Repository](https://github.com/rust-lang/mdBook)
- [Markdown Guide](https://www.markdownguide.org/)
