// rules: we always use `eprintln!()`
// we try to use ANSI escapes to color any noteworthy values, like paths, arguments, commands etc.
// messages have a _little_ personality and some cheer through emojis, but not too much.

use camino::Utf8Path;
use std::fmt;
use std::process::Command;

// Steps to follow:
// - A: ensure the existence of package.json
// - B: ensure that `package.json` has `type` set to `module`
// - C: ensure that `.gitignore` contains at least:
//   - `/node_modules`
//   - `/.home`
pub(crate) async fn perform_dev_setup(base_dir: &Utf8Path) -> eyre::Result<()> {
    let package_json_path = base_dir.join("package.json");
    if tokio::fs::metadata(&package_json_path).await.is_err() {
        eprintln!(
            "📦 \x1b[33mpackage.json\x1b[0m not found. Running \x1b[36m`pnpm init`\x1b[0m to create it..."
        );

        let output = Command::new("pnpm")
            .arg("init")
            .current_dir(base_dir)
            .output()?;

        if !output.status.success() {
            return Err(eyre::eyre!(
                "😕 Failed to run \x1b[36m`pnpm init`\x1b[0m: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        eprintln!("✅ Successfully created \x1b[33mpackage.json\x1b[0m");
    }

    // Ensure package.json has "type": "module"
    let package_json_content = tokio::fs::read_to_string(&package_json_path).await?;
    let mut package_json: serde_json::Value = serde_json::from_str(&package_json_content)?;

    if package_json["type"] != "module" {
        package_json["type"] = serde_json::Value::String("module".to_string());
        let updated_content = serde_json::to_string_pretty(&package_json)?;
        tokio::fs::write(&package_json_path, updated_content).await?;
        eprintln!(
            "🔄 Updated \x1b[33mpackage.json\x1b[0m to set \x1b[36m\"type\": \"module\"\x1b[0m"
        );
    }

    // Ensure .gitignore contains required entries
    let gitignore_path = base_dir.join(".gitignore");
    let mut gitignore_content = if tokio::fs::metadata(&gitignore_path).await.is_ok() {
        tokio::fs::read_to_string(&gitignore_path).await?
    } else {
        String::new()
    };

    let required_entries = vec!["/node_modules", "/.home"];
    let mut updated = false;

    for entry in required_entries {
        if !gitignore_content.contains(entry) {
            if !gitignore_content.is_empty() && !gitignore_content.ends_with('\n') {
                gitignore_content.push('\n');
            }
            gitignore_content.push_str(entry);
            gitignore_content.push('\n');
            updated = true;
        }
    }

    if updated {
        tokio::fs::write(&gitignore_path, gitignore_content).await?;
        eprintln!("📝 Updated \x1b[33m.gitignore\x1b[0m with required entries");
    }

    Ok(())
}

pub fn generate_config(tenant_name: &str) -> serde_json::Value {
    let random_cookie_sauce: String = std::iter::repeat_with(fastrand::alphanumeric)
        .take(64)
        .collect();

    serde_json::json!({
        "tenants": {
            tenant_name: {
                "base_dir": ".",
                "object_storage": {
                    "bucket": format!("home-assets-{}", tenant_name.replace('.', "-")),
                    "region": "nbg1",
                    "endpoint": "https://nbg1.your-objectstorage.com"
                },
                "secrets": {
                    "cookie_sauce": random_cookie_sauce,
                    "aws": {
                        "access_key_id": "FILL-ME",
                        "secret_access_key": "FILL-ME"
                    },
                    "patreon": {
                        "oauth_client_id": "FILL-ME",
                        "oauth_client_secret": "FILL-ME"
                    },
                    "github": {
                        "oauth_client_id": "FILL-ME",
                        "oauth_client_secret": "FILL-ME"
                    }
                },
            }
        },
        "disk_cache_size": "200 MiB",
        "env": "development",
        "address": "127.0.0.1:1111",
        "watch": true,
        "mom_base_url": "http://mom.snug.blog:1118",
        "secrets": {
            "mom": {
                "api_key": "mom_DUMMY_API_KEY"
            },
            "reddit": {
                "oauth_client_id": "FILL-ME",
                "oauth_client_secret": "FILL-ME"
            }
        }
    })
}

#[derive(Debug)]
struct FileInfo {
    path: String,
    content: String,
}

#[derive(Debug)]
struct ProjectChangeSet {
    files: Vec<FileInfo>,
}

impl ProjectChangeSet {
    fn new(config: serde_json::Value) -> eyre::Result<Self> {
        Ok(Self {
            files: vec![
                FileInfo {
                    path: "home.json".to_string(),
                    content: serde_json::to_string_pretty(&config)?,
                },
                FileInfo {
                    path: "content/_index.md".to_string(),
                    content: include_str!("scaffold/content/_index.md").to_string(),
                },
                FileInfo {
                    path: "templates/page.html.jinja".to_string(),
                    content: include_str!("scaffold/templates/page.html.jinja").to_string(),
                },
                FileInfo {
                    path: "src/bundle.ts".to_string(),
                    content: include_str!("scaffold/src/bundle.ts").to_string(),
                },
                FileInfo {
                    path: "src/main.scss".to_string(),
                    content: include_str!("scaffold/src/main.scss").to_string(),
                },
                FileInfo {
                    path: "src/_reset.scss".to_string(),
                    content: include_str!("scaffold/src/_reset.scss").to_string(),
                },
            ],
        })
    }

    async fn check_existing_files(&self, dir: &Utf8Path) -> eyre::Result<Vec<String>> {
        let mut existing_files = Vec::new();
        for file in &self.files {
            let full_path = dir.join(&file.path);
            if tokio::fs::metadata(&full_path).await.is_ok() {
                existing_files.push(file.path.clone());
            }
        }
        Ok(existing_files)
    }

    async fn commit(&self, dir: &Utf8Path) -> eyre::Result<()> {
        for file in &self.files {
            let full_path = dir.join(&file.path);
            if let Some(parent) = full_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&full_path, &file.content).await?;
            eprintln!("📄 Created file: \x1b[36m{full_path}\x1b[0m");
        }
        Ok(())
    }
}

impl fmt::Display for ProjectChangeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\x1b[34m📋 The following files will be created:\x1b[0m")?;
        for file in &self.files {
            writeln!(f, "  \x1b[36m{}\x1b[0m", file.path)?;
        }
        Ok(())
    }
}

pub async fn init_project(dir: &camino::Utf8Path, force: bool) -> eyre::Result<()> {
    let absolute_dir = dir
        .canonicalize_utf8()
        .map_err(|e| eyre::eyre!("Failed to get absolute path: {}", e))?;
    let tenant_name = absolute_dir
        .file_name()
        .ok_or_else(|| eyre::eyre!(
            "Invalid directory name: '{}'. Failed to extract the tenant name from the project directory.",
            absolute_dir
        ))?;

    let config = generate_config(tenant_name);
    let change_set = ProjectChangeSet::new(config)?;

    let existing_files = change_set.check_existing_files(dir).await?;

    if !existing_files.is_empty() && !force {
        eprintln!("\x1b[33m⚠️ The following files already exist:\x1b[0m");
        for file in &existing_files {
            eprintln!("  \x1b[36m{file}\x1b[0m");
        }
        eprintln!(
            "\x1b[33mPlease remove these files or run again with --force to overwrite.\x1b[0m"
        );
        std::process::exit(1);
    }

    // Ask for user consent
    if !force {
        println!("{change_set}");
        eprint!("\x1b[32mDo you want to proceed? (y/N): \x1b[0m");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            eprintln!("\x1b[33mOperation cancelled by user.\x1b[0m");
            std::process::exit(1);
        }
    }

    change_set.commit(dir).await?;

    eprintln!("\x1b[32m✨ Created initial content and source files! 🎉\x1b[0m");
    perform_dev_setup(dir).await?;
    eprintln!("\x1b[32m🚀 Development setup completed successfully! 🎊\x1b[0m");

    eprintln!("\n\x1b[33m=== 🌟 You're all set! 🌟 ===\x1b[0m");
    eprintln!(
        "\x1b[34m📌 Next step:\x1b[0m Run \x1b[36m`home serve`\x1b[0m to start the development server."
    );
    eprintln!("\x1b[32m🎈 Happy coding! 🎈\x1b[0m");

    Ok(())
}
