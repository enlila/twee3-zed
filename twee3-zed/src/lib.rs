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

            zed::download_file(
                &asset.download_url,
                &binary_path,
                zed::DownloadedFileType::Uncompressed,
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path).map_err(|e| format!("failed to make file executable: {e}"))?;
        }

        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env: Default::default(),
        })
    }
}

zed::register_extension!(Twee3Extension);
