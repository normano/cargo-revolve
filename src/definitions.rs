use serde::Serialize;

/// Data from the `[package]` section of Cargo.toml, passed to the template.
#[derive(Serialize)]
pub struct PkgContext<'a> {
  pub name: &'a str,
  pub version: &'a str,
  pub description: Option<&'a str>,
  pub license: Option<&'a str>,
}

/// Data from the `[package.metadata.revolve]` section, passed to the template.
#[derive(Serialize)]
pub struct BuilderContext<'a> {
  pub spec_template: &'a str,
}

/// The top-level context object passed to the Tera templating engine.
#[derive(Serialize)]
pub struct TemplateContext<'a> {
  pub pkg: PkgContext<'a>,
  pub builder: BuilderContext<'a>,
}