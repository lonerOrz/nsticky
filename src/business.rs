use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;

use crate::system_integration::WorkspaceRef;

#[derive(Clone)]
pub struct BusinessLogic {
    sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
    staged_set: std::sync::Arc<Mutex<HashMap<u64, bool>>>,
}

impl BusinessLogic {
    pub fn new() -> Self {
        Self {
            sticky_windows: std::sync::Arc::new(Mutex::new(HashSet::new())),
            staged_set: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        let mut staged = self.staged_set.lock().await;
        staged.remove(&window_id);
        Ok(sticky.insert(window_id))
    }

    pub async fn remove_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        let staged = self.staged_set.lock().await;
        if staged.contains_key(&window_id) {
            return Err(anyhow::anyhow!(
                "Window is in stage, cannot remove from sticky"
            ));
        }
        Ok(sticky.remove(&window_id))
    }

    pub async fn list_sticky_windows(&self) -> Result<Vec<u64>> {
        let snapshot: Vec<u64> = {
            let sticky = self.sticky_windows.lock().await;
            sticky.iter().copied().collect()
        };
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        Ok(snapshot
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect())
    }

    pub async fn toggle_active_window(&self) -> Result<bool> {
        let active_id = crate::system_integration::get_active_window_id().await?;
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&active_id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        let mut staged = self.staged_set.lock().await;

        if staged.contains_key(&active_id) {
            let current_ws_id = crate::system_integration::get_active_workspace_id().await?;
            crate::system_integration::move_to_workspace(
                active_id,
                WorkspaceRef::Id(current_ws_id),
            )
            .await?;
            staged.remove(&active_id);
            sticky.insert(active_id);
            Ok(true)
        } else if sticky.contains(&active_id) {
            sticky.remove(&active_id);
            Ok(false)
        } else {
            sticky.insert(active_id);
            Ok(true)
        }
    }

    pub async fn toggle_by_appid(&self, appid: &str) -> Result<usize> {
        let window_ids = crate::system_integration::find_windows_by_appid(appid).await?;
        if window_ids.is_empty() {
            return Err(anyhow::anyhow!("No window found with appid {}", appid));
        }
        self.toggle_windows(window_ids).await
    }

    pub async fn toggle_by_title(&self, title: &str) -> Result<usize> {
        let window_ids = crate::system_integration::find_windows_by_title(title).await?;
        if window_ids.is_empty() {
            return Err(anyhow::anyhow!("No window found with title {}", title));
        }
        self.toggle_windows(window_ids).await
    }

    async fn toggle_windows(&self, window_ids: Vec<u64>) -> Result<usize> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let current_ws_id = crate::system_integration::get_active_workspace_id().await?;
        let mut count = 0;

        for id in window_ids {
            if !full_window_list.contains(&id) {
                continue;
            }

            let (is_staged, is_sticky) = {
                let sticky = self.sticky_windows.lock().await;
                let staged = self.staged_set.lock().await;
                (staged.contains_key(&id), sticky.contains(&id))
            };

            if is_staged {
                crate::system_integration::move_to_workspace(id, WorkspaceRef::Id(current_ws_id))
                    .await?;
                let mut staged = self.staged_set.lock().await;
                let was_sticky = staged.remove(&id).unwrap_or(false);
                if was_sticky {
                    let mut sticky = self.sticky_windows.lock().await;
                    sticky.insert(id);
                }
            } else if is_sticky {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.remove(&id);
            } else {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.insert(id);
            }
            count += 1;
        }
        Ok(count)
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

        if let Err(e) =
            crate::system_integration::move_to_workspace(window_id, WorkspaceRef::Name("stage"))
                .await
        {
            let mut sticky = self.sticky_windows.lock().await;
            if was_sticky {
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
        self.stage_window(id).await
    }

    pub async fn is_window_staged(&self, window_id: u64) -> bool {
        let staged = self.staged_set.lock().await;
        staged.contains_key(&window_id)
    }

    pub async fn is_window_sticky(&self, window_id: u64) -> bool {
        let sticky = self.sticky_windows.lock().await;
        sticky.contains(&window_id)
    }

    pub async fn toggle_stage_by_appid(&self, appid: &str, workspace_id: u64) -> Result<usize> {
        let window_ids = crate::system_integration::find_windows_by_appid(appid).await?;
        if window_ids.is_empty() {
            return Err(anyhow::anyhow!("No window found with appid {}", appid));
        }
        self.toggle_stage_windows(window_ids, workspace_id).await
    }

    pub async fn toggle_stage_by_title(&self, title: &str, workspace_id: u64) -> Result<usize> {
        let window_ids = crate::system_integration::find_windows_by_title(title).await?;
        if window_ids.is_empty() {
            return Err(anyhow::anyhow!("No window found with title {}", title));
        }
        self.toggle_stage_windows(window_ids, workspace_id).await
    }

    async fn toggle_stage_windows(&self, window_ids: Vec<u64>, workspace_id: u64) -> Result<usize> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let mut count = 0;

        for id in window_ids {
            if !full_window_list.contains(&id) {
                continue;
            }

            if self.is_window_staged(id).await {
                self.unstage_window(id, workspace_id).await?;
            } else {
                self.stage_window(id).await?;
            }
            count += 1;
        }
        Ok(count)
    }

    pub async fn stage_all_windows(&self) -> Result<usize> {
        let valid_sticky_ids: Vec<u64> = {
            let sticky = self.sticky_windows.lock().await;
            let full_window_list = crate::system_integration::get_full_window_list().await?;
            sticky
                .iter()
                .copied()
                .filter(|id| full_window_list.contains(id))
                .collect()
        };

        if valid_sticky_ids.is_empty() {
            return Ok(0);
        }

        let mut results: Vec<(u64, bool)> = Vec::new();

        for id in valid_sticky_ids {
            let was_sticky = {
                let mut sticky = self.sticky_windows.lock().await;
                sticky.remove(&id)
            };

            if crate::system_integration::move_to_workspace(id, WorkspaceRef::Name("stage"))
                .await
                .is_ok()
            {
                results.push((id, was_sticky));
            } else {
                tracing::error!("Failed to move window {id} to stage");
                let mut sticky = self.sticky_windows.lock().await;
                if was_sticky {
                    sticky.insert(id);
                }
            }
        }

        let count = results.len();

        let mut staged = self.staged_set.lock().await;
        for (id, was_sticky) in results {
            staged.insert(id, was_sticky);
        }

        Ok(count)
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

        if let Err(e) =
            crate::system_integration::move_to_workspace(window_id, WorkspaceRef::Id(workspace_id))
                .await
        {
            let mut sticky = self.sticky_windows.lock().await;
            let mut staged = self.staged_set.lock().await;
            staged.insert(window_id, previously_sticky);
            if previously_sticky {
                sticky.insert(window_id);
            }
            return Err(e);
        }

        let mut sticky = self.sticky_windows.lock().await;
        if previously_sticky {
            sticky.insert(window_id);
        }
        Ok(())
    }

    pub async fn unstage_active_window(&self, workspace_id: u64) -> Result<()> {
        let id = crate::system_integration::get_active_window_id().await?;
        self.unstage_window(id, workspace_id).await
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
            .zip(previously_sticky_map)
            .filter(|(id, _)| full_window_list.contains(id))
            .collect();

        let mut results: Vec<(u64, bool)> = Vec::new();

        for (id, was_sticky) in valid_ids {
            if crate::system_integration::move_to_workspace(id, WorkspaceRef::Id(workspace_id))
                .await
                .is_ok()
            {
                results.push((id, was_sticky));
            } else {
                tracing::error!("Failed to move window {id} to workspace {workspace_id}");
                let mut staged = self.staged_set.lock().await;
                staged.insert(id, was_sticky);
            }
        }

        let count = results.len();

        let mut sticky = self.sticky_windows.lock().await;
        for (id, was_sticky) in results {
            if was_sticky {
                sticky.insert(id);
            }
        }

        Ok(count)
    }

    pub async fn handle_workspace_activation(&self, ws_id: u64) -> Result<()> {
        let sticky_snapshot = {
            let mut sticky = self.sticky_windows.lock().await;
            let staged = self.staged_set.lock().await;
            let full_window_list = crate::system_integration::get_full_window_list()
                .await
                .unwrap_or_default();
            sticky.retain(|win_id| full_window_list.contains(win_id));
            tracing::info!("Updated sticky windows: {:?}", *sticky);
            (sticky.clone(), staged.clone())
        };

        for win_id in sticky_snapshot.0.iter() {
            if let Err(e) =
                crate::system_integration::move_to_workspace(*win_id, WorkspaceRef::Id(ws_id)).await
            {
                tracing::error!("Failed to move window {win_id}: {e:?}");
            }
        }

        Ok(())
    }

    pub async fn remove_window_unconditionally(&self, window_id: u64) -> Result<()> {
        let mut sticky = self.sticky_windows.lock().await;
        let mut staged = self.staged_set.lock().await;
        sticky.remove(&window_id);
        staged.remove(&window_id);
        Ok(())
    }
}
