use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct BusinessLogic {
    sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
    // staged_set: window_id -> previously_sticky (true if was in sticky before staging)
    staged_set: std::sync::Arc<Mutex<HashMap<u64, bool>>>,
}

impl BusinessLogic {
    pub fn new(
        sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
        staged_set: std::sync::Arc<Mutex<HashMap<u64, bool>>>,
    ) -> Self {
        Self {
            sticky_windows,
            staged_set,
        }
    }

    pub async fn add_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        // If window is in stage, remove it from stage first
        {
            let mut staged = self.staged_set.lock().await;
            staged.remove(&window_id);
        }

        let mut sticky = self.sticky_windows.lock().await;
        Ok(sticky.insert(window_id))
    }

    pub async fn remove_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        // Cannot remove if window is in stage (S2 or S3)
        let is_staged = {
            let staged = self.staged_set.lock().await;
            staged.contains_key(&window_id)
        };
        if is_staged {
            return Err(anyhow::anyhow!(
                "Window is in stage, cannot remove from sticky"
            ));
        }

        let mut sticky = self.sticky_windows.lock().await;
        Ok(sticky.remove(&window_id))
    }

    pub async fn list_sticky_windows(&self) -> Result<Vec<u64>> {
        let snapshot: Vec<u64> = {
            let sticky = self.sticky_windows.lock().await;
            sticky.iter().copied().collect()
        };
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_snapshot: Vec<u64> = snapshot
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect();
        Ok(valid_snapshot)
    }

    pub async fn toggle_active_window(&self) -> Result<bool> {
        let active_id = crate::system_integration::get_active_window_id().await?;
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&active_id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        // Check if in stage
        let is_staged = {
            let staged = self.staged_set.lock().await;
            staged.contains_key(&active_id)
        };

        if is_staged {
            // If in stage, move to current workspace and add to sticky (S2→S1, S3→S1)
            let current_ws_id = crate::system_integration::get_active_workspace_id().await?;
            crate::system_integration::move_to_workspace(active_id, current_ws_id).await?;

            let mut staged = self.staged_set.lock().await;
            staged.remove(&active_id);

            let mut sticky = self.sticky_windows.lock().await;
            sticky.insert(active_id);
            Ok(true)
        } else {
            // Normal toggle
            let mut sticky = self.sticky_windows.lock().await;
            if sticky.contains(&active_id) {
                sticky.remove(&active_id);
                Ok(false)
            } else {
                sticky.insert(active_id);
                Ok(true)
            }
        }
    }

    pub async fn toggle_by_appid(&self, appid: &str) -> Result<bool> {
        let window_id = crate::system_integration::find_window_by_appid(appid).await?;
        match window_id {
            Some(id) => {
                let full_window_list = crate::system_integration::get_full_window_list().await?;
                if !full_window_list.contains(&id) {
                    return Err(anyhow::anyhow!(
                        "Window with appid {} not found in Niri",
                        appid
                    ));
                }

                // Check if in stage
                let is_staged = {
                    let staged = self.staged_set.lock().await;
                    staged.contains_key(&id)
                };

                if is_staged {
                    // If in stage, move to current workspace and add to sticky (S2→S1, S3→S1)
                    let current_ws_id =
                        crate::system_integration::get_active_workspace_id().await?;
                    crate::system_integration::move_to_workspace(id, current_ws_id).await?;

                    let mut staged = self.staged_set.lock().await;
                    staged.remove(&id);

                    let mut sticky = self.sticky_windows.lock().await;
                    sticky.insert(id);
                    Ok(true)
                } else {
                    // Normal toggle
                    let mut sticky = self.sticky_windows.lock().await;
                    if sticky.contains(&id) {
                        sticky.remove(&id);
                        Ok(false)
                    } else {
                        sticky.insert(id);
                        Ok(true)
                    }
                }
            }
            None => Err(anyhow::anyhow!("No window found with appid {}", appid)),
        }
    }

    pub async fn toggle_by_title(&self, title: &str) -> Result<bool> {
        let window_id = crate::system_integration::find_window_by_title(title).await?;
        match window_id {
            Some(id) => {
                let full_window_list = crate::system_integration::get_full_window_list().await?;
                if !full_window_list.contains(&id) {
                    return Err(anyhow::anyhow!(
                        "Window with title containing '{}' not found in Niri",
                        title
                    ));
                }

                // Check if in stage
                let is_staged = {
                    let staged = self.staged_set.lock().await;
                    staged.contains_key(&id)
                };

                if is_staged {
                    // If in stage, move to current workspace and add to sticky (S2→S1, S3→S1)
                    let current_ws_id =
                        crate::system_integration::get_active_workspace_id().await?;
                    crate::system_integration::move_to_workspace(id, current_ws_id).await?;

                    let mut staged = self.staged_set.lock().await;
                    staged.remove(&id);

                    let mut sticky = self.sticky_windows.lock().await;
                    sticky.insert(id);
                    Ok(true)
                } else {
                    // Normal toggle
                    let mut sticky = self.sticky_windows.lock().await;
                    if sticky.contains(&id) {
                        sticky.remove(&id);
                        Ok(false)
                    } else {
                        sticky.insert(id);
                        Ok(true)
                    }
                }
            }
            None => Err(anyhow::anyhow!(
                "No window found with title containing '{}'",
                title
            )),
        }
    }

    pub async fn stage_window(&self, window_id: u64) -> Result<()> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let was_sticky = {
            let mut sticky = self.sticky_windows.lock().await;
            let staged = self.staged_set.lock().await;

            if staged.contains_key(&window_id) {
                return Ok(());
            }

            let was = sticky.contains(&window_id);
            if was {
                sticky.remove(&window_id);
            }
            was
        };

        if let Err(e) = crate::system_integration::move_to_named_workspace(window_id, "stage").await
        {
            if was_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(window_id);
            }
            return Err(e);
        }

        let mut staged = self.staged_set.lock().await;
        staged.insert(window_id, was_sticky);
        Ok(())
    }

    pub async fn stage_active_window(&self) -> Result<()> {
        let id = crate::system_integration::get_active_window_id().await?;

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let was_sticky = {
            let mut sticky = self.sticky_windows.lock().await;
            let staged = self.staged_set.lock().await;

            if staged.contains_key(&id) {
                return Ok(());
            }

            let was = sticky.contains(&id);
            if was {
                sticky.remove(&id);
            }
            was
        };

        if let Err(e) = crate::system_integration::move_to_named_workspace(id, "stage").await {
            if was_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(id);
            }
            return Err(e);
        }

        let mut staged = self.staged_set.lock().await;
        staged.insert(id, was_sticky);
        Ok(())
    }

    pub async fn is_window_staged(&self, window_id: u64) -> bool {
        let staged = self.staged_set.lock().await;
        staged.contains_key(&window_id)
    }

    pub async fn toggle_stage_by_appid(&self, appid: &str, workspace_id: u64) -> Result<()> {
        let window_id = crate::system_integration::find_window_by_appid(appid).await?;
        let id = match window_id {
            Some(id) => id,
            None => return Err(anyhow::anyhow!("No window found with appid {}", appid)),
        };
        let is_staged = self.is_window_staged(id).await;

        if is_staged {
            self.unstage_window(id, workspace_id).await
        } else {
            self.stage_window(id).await
        }
    }

    pub async fn toggle_stage_by_title(&self, title: &str, workspace_id: u64) -> Result<()> {
        let window_id = crate::system_integration::find_window_by_title(title).await?;
        let id = match window_id {
            Some(id) => id,
            None => return Err(anyhow::anyhow!("No window found with title {}", title)),
        };
        let is_staged = self.is_window_staged(id).await;

        if is_staged {
            self.unstage_window(id, workspace_id).await
        } else {
            self.stage_window(id).await
        }
    }

    pub async fn stage_all_windows(&self) -> Result<usize> {
        let sticky_ids = {
            let sticky = self.sticky_windows.lock().await;
            sticky.clone()
        };

        if sticky_ids.is_empty() {
            return Ok(0);
        }

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_sticky_ids: Vec<u64> = sticky_ids
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect();

        let mut successfully_staged = Vec::new();
        let mut failed_ids = Vec::new();

        for id in valid_sticky_ids {
            let was_sticky = {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.remove(&id)
            };

            if crate::system_integration::move_to_named_workspace(id, "stage")
                .await
                .is_ok()
            {
                successfully_staged.push((id, was_sticky));
            } else {
                eprintln!("Failed to move window {} to stage", id);
                failed_ids.push((id, was_sticky));
            }
        }

        // Restore failed windows to sticky
        for (id, was_sticky) in failed_ids {
            if was_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(id);
            }
        }

        let mut staged = self.staged_set.lock().await;
        for (id, was_sticky) in successfully_staged.drain(..) {
            staged.insert(id, was_sticky);
        }

        Ok(successfully_staged.len())
    }

    pub async fn list_staged_windows(&self) -> Result<Vec<u64>> {
        let staged = self.staged_set.lock().await;
        Ok(staged.keys().copied().collect())
    }

    pub async fn unstage_window(&self, window_id: u64, workspace_id: u64) -> Result<()> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let previously_sticky = {
            let mut staged = self.staged_set.lock().await;
            match staged.remove(&window_id) {
                Some(v) => v,
                None => return Err(anyhow::anyhow!("Window is not in staged list")),
            }
        };

        if let Err(e) = crate::system_integration::move_to_workspace(window_id, workspace_id).await
        {
            let mut staged = self.staged_set.lock().await;
            staged.insert(window_id, previously_sticky);
            if previously_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(window_id);
            }
            return Err(e);
        }

        if previously_sticky {
            let mut sticky = self.sticky_windows.lock().await;
            sticky.insert(window_id);
        }

        Ok(())
    }

    pub async fn unstage_active_window(&self, workspace_id: u64) -> Result<()> {
        let id = crate::system_integration::get_active_window_id().await?;

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let previously_sticky = {
            let mut staged = self.staged_set.lock().await;
            match staged.remove(&id) {
                Some(v) => v,
                None => return Err(anyhow::anyhow!("Active window is not in staged list")),
            }
        };

        if let Err(e) = crate::system_integration::move_to_workspace(id, workspace_id).await {
            let mut staged = self.staged_set.lock().await;
            staged.insert(id, previously_sticky);
            if previously_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(id);
            }
            return Err(e);
        }

        if previously_sticky {
            let mut sticky = self.sticky_windows.lock().await;
            sticky.insert(id);
        }

        Ok(())
    }

    pub async fn unstage_all_windows(&self, workspace_id: u64) -> Result<usize> {
        let (ids_to_unstage, previously_sticky_map): (Vec<u64>, Vec<bool>) = {
            let mut staged = self.staged_set.lock().await;
            if staged.is_empty() {
                return Ok(0);
            }
            let ids: Vec<u64> = staged.keys().copied().collect();
            let was_sticky: Vec<bool> = staged.values().copied().collect();
            staged.clear();
            (ids, was_sticky)
        };

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_ids: Vec<(u64, bool)> = ids_to_unstage
            .into_iter()
            .zip(previously_sticky_map.into_iter())
            .filter(|(id, _)| full_window_list.contains(id))
            .collect();

        let mut successfully_unstaged = Vec::new();
        let mut failed_ids = Vec::new();

        for (id, was_sticky) in valid_ids {
            if crate::system_integration::move_to_workspace(id, workspace_id)
                .await
                .is_ok()
            {
                successfully_unstaged.push((id, was_sticky));
            } else {
                eprintln!("Failed to move window {} to workspace {}", id, workspace_id);
                failed_ids.push((id, was_sticky));
            }
        }

        // Restore failed windows to staged
        let mut staged = self.staged_set.lock().await;
        for (id, was_sticky) in failed_ids {
            staged.insert(id, was_sticky);
        }

        // Handle successfully unstaged windows
        let mut sticky = self.sticky_windows.lock().await;
        let count = successfully_unstaged.len();

        for (id, was_sticky) in successfully_unstaged {
            // Don't put back to staged - it's now unstaged
            if was_sticky {
                sticky.insert(id);
            }
        }

        Ok(count)
    }

    pub async fn handle_workspace_activation(&self, ws_id: u64) -> Result<()> {
        let sticky_snapshot = {
            let mut sticky = self.sticky_windows.lock().await;
            let full_window_list = crate::system_integration::get_full_window_list()
                .await
                .unwrap_or_default();
            sticky.retain(|win_id| full_window_list.contains(win_id));
            println!("Updated sticky windows: {:?}", *sticky);
            sticky.clone()
        };

        for win_id in sticky_snapshot.iter() {
            if let Err(_e) = crate::system_integration::move_to_workspace(*win_id, ws_id).await {
                eprintln!("Failed to move window {}: {:?}", win_id, _e);
            }
        }

        Ok(())
    }
}
