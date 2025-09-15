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
- **Automatic Directory Expansion:** Intelligently include the contents of entire directories by adding a trailing slash to the `source` path in your `assets` list (e.g., `"config/"`). `cargo-revolve` will recursively find all files and map them to the correct destinations in the final RPM.
- **Automatic Directory Ownership:** The tool automatically discovers all directories required by your assets and generates the necessary `%dir` directives in the spec file, ensuring correct ownership.
- **Safe System Directories:** For assets installed into shared system directories (e.g., `/usr/lib/systemd/system`), you can use the `mkdir = false` flag to prevent the package from dangerously taking ownership of them.
- **Custom Build Command:** Replace the default `cargo build` with your own build script or command (e.g., `cargo leptos build`), perfect for projects with complex build steps like WebAssembly or CSS processing.
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

    Create a template file (e.g., `.revolve/my-app.spec.in`). This template is now fully automatic, leveraging the tool's directory discovery.

    ```spec
    # Disable automatic debug package generation.
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
    %setup -q -n {{ builder.archive_root_dir }}

    %build
    # This section is intentionally empty.

    %install
    rm -rf %{buildroot}
    # This simple loop works for all assets because the tool expands directories.
    {% for asset in builder.assets %}
    install -D -m {{ asset.mode | default(value="0644") }} "{{ asset.source | split(pat="/") | last }}" "%{buildroot}{{ asset.dest }}"
    {% endfor %}

    %files
    %defattr(-, root, root, -)
    # This loop automatically adds all necessary %dir directives.
    {% if builder.created_dirs %}{% for dir in builder.created_dirs %}
    %dir {{ dir }}
    {% endfor %}{% endif %}

    # This loop lists all the individual files.
    {% for asset in builder.assets %}
    {{ asset.dest }}
    {% endfor %}

    # Conditionally include the changelog.
    {% if builder.changelog %}
    %changelog
    {{ builder.changelog | trim }}
    {% endif %}
    ```

2.  **Configure `Cargo.toml`**

    Add a `[package.metadata.revolve]` section to your `Cargo.toml`.

    ```toml
    [package.metadata.revolve]
    spec_template = ".revolve/my-app.spec.in"
    output_dir = "dist"
    changelog = "CHANGELOG.md"

    # List all asset files and directories to be included in the RPM.
    assets = [
      # The compiled binary (a file). Its parent directory (/usr/bin) will be created.
      { source = "target/release/my-app", dest = "/usr/bin/my-app", mode = "0755" },
      
      # A systemd service file. We MUST NOT own its parent directory.
      { source = "systemd/my-app.service", dest = "/usr/lib/systemd/system/my-app.service", mkdir = false },
      
      # Include an entire configuration directory.
      # The trailing slash tells cargo-revolve to expand this into individual file assets.
      # The parent directories (/etc/my-app and /etc/my-app/conf.d) will be automatically owned.
      { source = "config/", dest = "/etc/my-app/conf.d/" },
    ]
    ```

3.  **Build!**

    Run the build command from the root of your project.

    ```bash
    cargo revolve build --verify
    ```

    Your newly built RPM(s) will be in the `dist/` directory.

## Advanced Usage: Custom Build Commands

For projects that require more than a simple `cargo build` (e.g., web frontends using tools like `cargo-leptos`, or projects requiring code generation), you can specify a custom `build_command`.

This feature replaces the default build step with commands of your choosing. It works in tandem with the `--no-archive` flag to package artifacts directly from your `target` directory.

1.  **Configure `Cargo.toml` with `build_command`**

    The `build_command` key can be a single command string or an array of commands to be executed sequentially.

    ```toml
    [package.metadata.revolve]
    spec_template = ".revolve/leptos-app.spec.in"
    output_dir = "dist"
    
    # Run a custom build tool instead of `cargo build`.
    # cargo-revolve sets REVOLVE_TARGET_DIR, REVOLVE_PACKAGE_NAME, and REVOLVE_PACKAGE_VERSION
    # environment variables for your script to use.
    build_command = "cargo leptos build --release"

    # Define the assets created by the custom command.
    # The directory expansion and mkdir flags work here too!
    assets = [
      { source = "target/server/release/leptos-app", dest = "/usr/bin/leptos-app", mode = "0755" },
      { source = "target/site/", dest = "/var/www/leptos-app/" },
    ]
    ```

2.  **Create a `--no-archive` Compatible `.spec.in`**

    When using a custom build command, you must use a spec file designed for the `--no-archive` workflow. The automatic directory ownership feature works here as well.

    ```spec
    # This spec is for --no-archive builds with pre-built artifacts.
    Name:           {{ pkg.name }}
    Version:        {{ pkg.version }}
    Release:        1%{?dist}
    Summary:        A Leptos web application
    License:        MIT

    # This directive tells rpmbuild this is a binary-only package.
    %define _binary_payload w7.gzdio

    %description
    A web application built with Leptos.

    %prep
    %build

    %install
    rm -rf %{buildroot}
    # The simple loop works because cargo-revolve expands directory assets.
    {% for asset in builder.assets %}
    # Use the absolute path provided by _sourcedir to locate the artifact.
    install -D -m {{ asset.mode | default(value="0644") }} "%{_sourcedir}/{{ asset.source }}" "%{buildroot}{{ asset.dest }}"
    {% endfor %}

    %files
    %defattr(-, root, root, -)
    # The automatic %dir loop works perfectly here too.
    {% if builder.created_dirs %}{% for dir in builder.created_dirs %}
    %dir {{ dir }}
    {% endfor %}{% endif %}

    {% for asset in builder.assets %}
    {{ asset.dest }}
    {% endfor %}
    ```

3.  **Build with `--no-archive`**

    You must use the `--no-archive` flag when a `build_command` is specified.

    ```bash
    cargo revolve build --no-archive --verify
    ```

## Usage

```
cargo revolve <COMMAND>
```

### Commands

- `cargo revolve build [OPTIONS]`
  -   `--dry-run`: Prepare everything but skip the final `rpmbuild` execution. Prints the rendered `.spec` and the `rpmbuild` command that would be run.
  -   `--verify`: After building, inspect the main binary RPM to ensure its name, version, files, and permissions match your configuration.
  -   `--no-archive`: (Advanced) Build directly from the source tree without creating a source archive. This is the **required mode for custom `build_command` workflows** where artifacts are generated in the project's `target` directory. Requires a spec file that does not use the `%setup` macro and instead copies files from `%{_sourcedir}` in the `%install` section.

- `cargo revolve info <RPM_FILE>`
  -   Parses the given `.rpm` file and prints its metadata and file manifest.

## Contributing

This project is open to contributions! Please feel free to open an issue or submit a pull request.

## License

`cargo-revolve` is distributed under the terms of the MPL-2.0 license. See [LICENSE](LICENSE) for details.