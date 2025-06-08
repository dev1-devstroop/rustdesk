use std::path::PathBuf;
use std::fs;
use anyhow::Result;
use uuid::Uuid;

pub struct IsolationEnvironment {
    pub session_id: Uuid,
    pub base_dir: PathBuf,
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl IsolationEnvironment {
    pub fn new(session_id: Uuid) -> Result<Self> {
        let base_dir = PathBuf::from("/tmp/rustdesk-isolation").join(session_id.to_string());
        
        let home_dir = base_dir.join("home");
        let data_dir = base_dir.join("data");
        let config_dir = base_dir.join("config");
        let cache_dir = base_dir.join("cache");
        let temp_dir = base_dir.join("tmp");

        // Create all directories
        fs::create_dir_all(&home_dir)?;
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&cache_dir)?;
        fs::create_dir_all(&temp_dir)?;

        log::info!("Created isolation environment for session {} at {:?}", session_id, base_dir);

        Ok(Self {
            session_id,
            base_dir,
            home_dir,
            data_dir,
            config_dir,
            cache_dir,
            temp_dir,
        })
    }

    pub fn cleanup(&self) -> Result<()> {
        log::info!("Cleaning up isolation environment for session {}", self.session_id);
        
        if self.base_dir.exists() {
            fs::remove_dir_all(&self.base_dir)
                .map_err(|e| anyhow::anyhow!("Failed to remove isolation directory {:?}: {}", self.base_dir, e))?;
        }

        Ok(())
    }

    pub fn copy_shared_files(&self, source_paths: &[PathBuf]) -> Result<()> {
        for source_path in source_paths {
            if source_path.exists() {
                let file_name = source_path.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid file path: {:?}", source_path))?;
                
                let dest_path = self.home_dir.join(file_name);
                
                if source_path.is_dir() {
                    self.copy_dir_recursive(source_path, &dest_path)?;
                } else {
                    fs::copy(source_path, &dest_path)?;
                }
                
                log::debug!("Copied {:?} to {:?}", source_path, dest_path);
            }
        }
        Ok(())
    }

    fn copy_dir_recursive(&self, src: &PathBuf, dst: &PathBuf) -> Result<()> {
        fs::create_dir_all(dst)?;
        
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            
            if src_path.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }
        
        Ok(())
    }
}
