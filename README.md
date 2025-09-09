# `cargo-revolve`: The Power of RPM, the Convenience of Cargo

`cargo-revolve` is a Cargo subcommand for building RPM packages from Rust projects. It follows the robust "pre-compiled artifact" model, acting as an intelligent orchestrator for the native `rpmbuild` toolchain. It embraces the full power of the `.spec` file format while providing seamless integration with your `Cargo.toml`.

This tool is the spiritual successor to the original `cargo-rpm`, modernized for today's Rust development. It is for developers and packagers who need to create production-ready, distributable RPMs without requiring the target build environment to have a Rust toolchain installed.

[![Crates.io](https://img.shields.io/crates/v/cargo-revolve.svg)](https://crates.io/crates/cargo-revolve)
[![Docs.rs](https://docs.rs/cargo-revolve/badge.svg)](https://docs.rs/cargo-revolve)

## The `cargo-revolve` Philosophy

`cargo-revolve` believes in a clear separation of concerns:
1.  **`cargo` compiles your code.** It's the best tool for building Rust binaries.
2.  **`cargo-revolve` packages the results.** It gathers your compiled binaries and other assets into a source tarball.
3.  **`rpmbuild` builds the RPM.** It takes the tarball of pre-compiled artifacts and uses your `.spec` file to create the final package.

This workflow is fast, reliable, and does not require `rustc` or `cargo` on your RPM build servers, making it ideal for corporate environments and simplified CI/CD pipelines.

### How It Compares to Other Solutions

| Tool | Philosophy | Best For | `cargo-revolve` Advantage |
|---|---|---|---|
| **`cargo-generate-rpm`** | Declarative (`Cargo.toml` is everything) | Simple projects, CI environments without `rpmbuild`. | **Full Power & Control.** `cargo-revolve` doesn't hide the `.spec` file, giving you access to 100% of RPM's features like complex scriptlets, triggers, and custom macros. You are never limited by the tool's TOML schema. |
| **"Spec-driven Build"** (e.g., Fedora Packaging) | Let `rpmbuild` compile the code | Official distribution packaging. | **Simplicity & Portability.** `cargo-revolve`'s pre-compiled model means your build servers don't need a Rust toolchain. You ship binaries, not source, which is simpler and often required for proprietary applications. |
| **`cargo-dist`** | Full Release Orchestration | Multi-platform releases (RPM, DEB, MSI) automated in CI. | **Focus & Depth.** `cargo-dist` is a powerful release framework. `cargo-revolve` is a focused tool that does one thing exceptionally well: building robust, production-grade RPMs. |

**In short, `cargo-revolve` gives you the expressiveness of a raw `.spec` file with a workflow optimized for distributing compiled Rust applications.**

## Features

- **Pre-compiled Artifact Workflow:** Compiles your code locally first, then packages the results for `rpmbuild`.
- **`.spec` File Templating:** Uses the powerful [Tera](https://tera.netlify.app/) template engine to inject metadata from your `Cargo.toml` directly into your `.spec` file.
- **Data-Driven Packaging:** Define your package files once in an `assets` list in `Cargo.toml` and use loops in your template to automatically populate the `%install` and `%files` sections.
- **Automatic Changelog Inclusion:** Reads a changelog file and injects it directly into the spec's `%changelog` section.
- **Clean Output Directory:** Copies final RPMs to a user-defined directory (e.g., `dist/`) for easy access in CI/CD.
- **Workspace-Aware:** Correctly locates the `target` directory and package paths, whether in a single crate or a complex workspace.
- **Post-Build Verification:** The `--verify` flag parses the generated RPMs to ensure their contents and file permissions match your configuration, catching packaging errors instantly.
- **Native `rpmbuild` Backend:** Ensures 100% compatibility with all RPM features and build environments.
- **Developer-Friendly Workflow:** A `--dry-run` flag shows you exactly what would happen.
- **Built-in Inspector:** The `info` subcommand quickly inspects the metadata and file list of any `.rpm` file.

## Installation

```bash
cargo install cargo-revolve
```

Ensure that `rpmbuild` is installed on your system.
- On Fedora/RHEL/CentOS: `sudo dnf install rpm-build`
- On openSUSE: `sudo zypper install rpmbuild`

## Quick Start

1.  **Create a `.spec.in` Template**

    Create a template file (e.g., `.revolve/my-app.spec.in`). Note that the `%build` section is empty, as our binary is pre-compiled.

    ```spec
    # Disable automatic debug package generation, which is not needed for pre-compiled binaries.
    %define debug_package %{nil}

    Name:           {{ pkg.name }}
    Version:        {{ pkg.version }}
    Release:        1%{?dist}
    Summary:        {{ pkg.description }}
    License:        {{ pkg.license }}
    Source0:        {{ pkg.name }}-{{ pkg.version }}.tar.gz

    %description
    {{ pkg.description }}

    %prep
    # Use the variable provided by cargo-revolve for robustness.
    %setup -q -n {{ builder.archive_root_dir }}

    %build
    # This section is intentionally empty.

    %install
    rm -rf %{buildroot}
    # Loop over the assets from Cargo.toml and install them from the archive root.
    {% for asset in builder.assets %}
    install -D -m {{ asset.mode | default(value="0644") }} "{{ asset.source | split(pat="/") | last }}" "%{buildroot}{{ asset.dest }}"
    {% endfor %}

    %files
    %defattr(-, root, root, -)
    # List all the files the package owns.
    {% for asset in builder.assets %}
    {{ asset.dest }}
    {% endfor %}

    # Conditionally include the changelog if it's provided.
    {% if builder.changelog %}
    %changelog
    {{ builder.changelog | trim }}
    {% endif %}
    ```

2.  **Configure `Cargo.toml`**

    Add a `[package.metadata.revolve]` section to your `Cargo.toml`. The `source` paths must point to the locations of your assets *after* a successful `cargo build`.

    ```toml
    [package.metadata.revolve]
    # Path to the spec file template.
    spec_template = ".revolve/my-app.spec.in"
    
    # (Optional) Copy final RPMs to this directory.
    output_dir = "dist"
    
    # (Optional) Read this file's content into the template's `builder.changelog` variable.
    changelog = "CHANGELOG.md"

    # List all asset files to be included in the RPM.
    assets = [
      # The compiled binary from the target directory.
      { source = "target/release/my-app", dest = "/usr/bin/my-app", mode = "0755" },
      
      # A systemd service file from your project.
      { source = "systemd/my-app.service", dest = "/usr/lib/systemd/system/my-app.service" },
      
      # A default configuration file.
      { source = "config/default.toml", dest = "/etc/my-app/default.toml" },
    ]
    ```

3.  **Build!**

    Run the build command from the root of your project. `cargo-revolve` will run `cargo build` for you.

    ```bash
    cargo revolve build --verify
    ```

    Your newly built RPM(s) will be in the `dist/` directory, as configured in `Cargo.toml`.

## Usage

```
cargo revolve <COMMAND>
```

### Commands

- `cargo revolve build [OPTIONS]`
  -   `--dry-run`: Prepare everything but skip the final `rpmbuild` execution. Prints the rendered `.spec` and the `rpmbuild` command that would be run.
  -   `--verify`: After building, inspect the main binary RPM to ensure its name, version, files, and permissions match your configuration.
  -   `--no-archive`: (Advanced) Build directly from the source tree without creating a source archive. Requires a spec file that does not use the `%setup` macro.

- `cargo revolve info <RPM_FILE>`
  -   Parses the given `.rpm` file and prints its metadata and file manifest.

## Contributing

This project is open to contributions! Please feel free to open an issue or submit a pull request.

## License

`cargo-revolve` is distributed under the terms of the MPL-2.0 license. See [LICENSE](LICENSE) for details.