use serde::Serialize;

use crate::config::Asset;

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
  
  pub archive_root_dir: &'a str,
  
  #[serde(skip_serializing_if = "Option::is_none")]
  pub changelog: Option<&'a str>,
  
  #[serde(skip_serializing_if = "Option::is_none")]
  pub assets: Option<&'a Vec<Asset>>,
  
  #[serde(skip_serializing_if = "Option::is_none")]
  pub build_flags: Option<&'a Vec<String>>,
  
}

/// The top-level context object passed to the Tera templating engine.
#[derive(Serialize)]
pub struct TemplateContext<'a> {
  pub pkg: PkgContext<'a>,
  pub builder: BuilderContext<'a>,
}