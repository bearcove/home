use autotrait::autotrait;
use camino::{Utf8Path, Utf8PathBuf};
use config_types::{
    CubConfig, CubConfigBundle, MomConfig, RevisionConfig, TenantConfig, TenantDomain, TenantInfo,
};
use facet_pretty::FacetPretty;
use owo_colors::OwoColorize;
use std::collections::HashMap;

pub use camino;
pub use eyre::Result;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static INSTANCE: ModImpl = ModImpl;
    &INSTANCE
}

#[autotrait]
impl Mod for ModImpl {
    fn load_cub_config(
        &self,
        config_path: Option<&Utf8Path>,
        roots: Vec<Utf8PathBuf>,
    ) -> Result<CubConfigBundle> {
        if config_path.is_some() && !roots.is_empty() {
            if roots.len() == 1 && roots[0].as_str() == "." {
                // ignore, that's the default
            } else {
                eprintln!("Error: Please specify either a config file or tenant roots, not both.");
                eprintln!(
                    "You provided --config {config_path:?} and tenant roots {roots:?}"
                );
                eprintln!(
                    "Use either `serve --config cub-config.json` or `serve tenant1.com/ tenant2.org/ etc.`"
                );
                std::process::exit(1);
            }
        }

        if let Some(config_path) = config_path {
            eprintln!("Loading config from {config_path}");

            let file_contents = fs_err::read_to_string(config_path)?;
            let config: CubConfig = serde_json::from_str(&file_contents)?;
            return Ok(CubConfigBundle {
                cc: config,
                // those will be loaded from mom
                tenants: Default::default(),
            });
        }

        if roots.is_empty() {
            eprintln!("Error: Please specify either a config file or tenant roots.");
            eprintln!(
                "Use either `serve --config cub-config.json` or `serve tenant1.com/ tenant2.org/ etc.`"
            );
            std::process::exit(1);
        }

        eprintln!(
            "Loading empty config (got roots {})",
            roots
                .iter()
                .map(|root| root.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
        let cc: CubConfig = serde_json::from_str("{}")?;
        let mut bundle = CubConfigBundle {
            cc,
            tenants: HashMap::new(),
        };

        for root in roots {
            if !root.exists() {
                eprintln!("Error: Tenant root {root} does not exist.");
                std::process::exit(1);
            }

            let public_config_path = root.join("home.json");
            if !public_config_path.exists() {
                eprintln!(
                    "Error: Public config file {public_config_path} does not exist."
                );
                std::process::exit(1);
            }

            let config_contents = fs_err::read_to_string(&public_config_path)?;
            let config: RevisionConfig =
                facet_json::from_str(&config_contents).map_err(|e| eyre::eyre!("{e}"))?;
            eprintln!("Got config {}", config.pretty());

            let base_dir = root.canonicalize_utf8()?;
            let tenant = TenantDomain::new(config.id);
            let tc = TenantConfig {
                name: tenant.clone(),
                object_storage: None,
                domain_aliases: Default::default(),
                secrets: None,
            };
            let ti = TenantInfo { base_dir, tc };
            bundle.tenants.insert(tenant, ti);
        }
        Ok(bundle)
    }

    fn load_mom_config(&self, config_path: &Utf8Path) -> Result<MomConfig> {
        eprintln!("Reading config from {}", config_path.blue());
        let config_path = config_path.canonicalize_utf8()?;

        let config: MomConfig = serde_json::from_str(&fs_err::read_to_string(config_path)?)?;
        Ok(config)
    }
}
