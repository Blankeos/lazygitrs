use anyhow::Result;

use super::GitCommands;

#[derive(Debug, Clone)]
pub struct Submodule {
    pub name: String,
    pub path: String,
    pub url: String,
}

impl GitCommands {
    pub fn load_submodules(&self) -> Result<Vec<Submodule>> {
        let result = self.git().args(&["submodule", "status"]).run()?;
        if !result.success || result.stdout.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut submodules = Vec::new();
        for line in result.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Format: " hash path (describe)" or "+hash path (describe)" or "-hash path"
            let line = line.trim_start_matches([' ', '+', '-']);
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let path = parts[1].to_string();
                submodules.push(Submodule {
                    name: path.clone(),
                    path,
                    url: String::new(),
                });
            }
        }

        Ok(submodules)
    }

    pub fn init_submodules(&self) -> Result<()> {
        self.git()
            .args(&["submodule", "init"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn update_submodules(&self) -> Result<()> {
        self.git()
            .args(&["submodule", "update", "--init", "--recursive"])
            .run_expecting_success()?;
        Ok(())
    }
}
