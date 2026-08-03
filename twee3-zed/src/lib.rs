use zed_extension_api as zed;
use std::fs;

struct Twee3Extension {}

impl zed::Extension for Twee3Extension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let release = zed::latest_github_release(
            "enlila/twee3-zed",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();
        
        let asset_name = format!(
            "twee3-lsp-{os}-{arch}{extension}",
            os = match os {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X8664 => "x86_64",
                _ => return Err("Unsupported architecture".to_string()),
            },
            extension = match os {
                zed::Os::Windows => ".exe",
                _ => "",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("twee3-lsp-{}", release.version);
        let binary_path = format!("{version_dir}/{asset_name}");

        if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            fs::create_dir_all(&version_dir)
                .map_err(|e| format!("failed to create version directory: {e}"))?;

            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path).map_err(|e| format!("failed to make file executable: {e}"))?;
        }

        // --- Tweego Auto-Download Logic ---
        let tweego_release = zed::latest_github_release(
            "tmedwards/tweego",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let version_without_v = tweego_release.version.strip_prefix('v').unwrap_or(&tweego_release.version);
        let tweego_asset_name = format!(
            "tweego-{version}-{os}-{arch}.zip",
            version = version_without_v,
            os = match os {
                zed::Os::Mac => "macos",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "windows",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "x64", // Tweego doesn't have native ARM, x64 works via Rosetta/Rosetta2
                zed::Architecture::X8664 => "x64",
                _ => return Err("Unsupported architecture for Tweego".to_string()),
            }
        );

        let tweego_asset = tweego_release
            .assets
            .iter()
            .find(|asset| asset.name == tweego_asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", tweego_asset_name))?;

        let tweego_version_dir = format!("tweego-{}", tweego_release.version);
        let tweego_binary_path = format!("{tweego_version_dir}/tweego{extension}", extension = match os {
            zed::Os::Windows => ".exe",
            _ => "",
        });

        if !fs::metadata(&tweego_binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            fs::create_dir_all(&tweego_version_dir)
                .map_err(|e| format!("failed to create tweego version directory: {e}"))?;

            zed::download_file(
                &tweego_asset.download_url,
                &tweego_version_dir,
                zed::DownloadedFileType::Zip,
            )
            .map_err(|e| format!("failed to download Tweego: {e}"))?;

            zed::make_file_executable(&tweego_binary_path).map_err(|e| format!("failed to make Tweego executable: {e}"))?;
        }

        let env = vec![("TWEEGO_PATH".to_string(), tweego_binary_path)];

        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env,
        })
    }
}

zed::register_extension!(Twee3Extension);
