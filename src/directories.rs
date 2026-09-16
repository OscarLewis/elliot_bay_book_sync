use std::{path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub struct AppDirs {
    pub config_dir: Arc<Path>,
    pub data_dir: Arc<Path>,
    pub image_dir: Arc<Path>,
}

impl AppDirs {
    pub fn system() -> Self {
        Self {
            config_dir: Arc::from(Path::new("/etc/ebbooks")),
            data_dir: Arc::from(Path::new("/var/lib/ebbooks")),
            image_dir: Arc::from(Path::new("/var/lib/ebbooks/images")),
        }
    }

    pub fn user() -> Self {
        let dirs = directories::ProjectDirs::from("", "", "ebbooks")
            .expect("could not determine user directories");

        let config_dir: Arc<Path> = Arc::from(dirs.config_dir());
        let data_dir: Arc<Path> = Arc::from(dirs.data_dir());
        let image_dir: Arc<Path> = Arc::from(data_dir.join("images").as_path());

        Self {
            config_dir,
            data_dir,
            image_dir,
        }
    }

    pub fn create(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.image_dir)?;

        Ok(())
    }
}
