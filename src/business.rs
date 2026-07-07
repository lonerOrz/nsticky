use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;

use crate::protocol;
use crate::system_integration::WorkspaceRef;

#[derive(Clone)]
pub struct BusinessLogic {
    sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
    staged_set: std::sync::Arc<Mutex<HashMap<u64, bool>>>,
    auto_staged_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
    config: std::sync::Arc<crate::config::Config>,
}

impl BusinessLogic {
    pub fn new(config: crate::config::Config) -> Self {
        Self {
            sticky_windows: std::sync::Arc::new(Mutex::new(HashSet::new())),
            staged_set: std::sync::Arc::new(Mutex::new(HashMap::new())),
            auto_staged_windows: std::sync::Arc::new(Mutex::new(HashSet::new())),
            config: std::sync::Arc::new(config),
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

    pub async fn handle_window_opened_or_changed(
        &self,
        id: u64,
        app_id: Option<String>,
        title: Option<String>,
    ) -> Result<()> {
        let is_in_staged = self.is_window_staged(id).await;
        let is_in_sticky = self.is_window_sticky(id).await;

        let was_auto_sticky = {
            let auto_staged = self.auto_staged_windows.lock().await;
            auto_staged.contains(&id)
        };

        if was_auto_sticky && (!is_in_sticky || is_in_staged) {
            return Ok(());
        }

        if self.config.match_sticky(&app_id, &title) {
            tracing::info!("Auto-sticky window {id} ({app_id:?})");
            self.add_sticky_window(id).await?;
            let mut auto_staged = self.auto_staged_windows.lock().await;
            auto_staged.insert(id);
        }

        Ok(())
    }

    pub async fn handle_window_closed(&self, id: u64) -> Result<()> {
        self.remove_window_unconditionally(id).await
    }

    pub async fn remove_window_unconditionally(&self, window_id: u64) -> Result<()> {
        let mut sticky = self.sticky_windows.lock().await;
        let mut staged = self.staged_set.lock().await;
        let mut auto_staged = self.auto_staged_windows.lock().await;
        sticky.remove(&window_id);
        staged.remove(&window_id);
        auto_staged.remove(&window_id);
        Ok(())
    }

    pub async fn handle_request(&self, request: protocol::Request) -> protocol::Response {
        match request {
            protocol::Request::Add { window_id } => match self.add_sticky_window(window_id).await {
                Ok(is_new) => {
                    if is_new {
                        protocol::Response::Success {
                            message: "Added".to_string(),
                        }
                    } else {
                        protocol::Response::Success {
                            message: "Already in sticky list".to_string(),
                        }
                    }
                }
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            },
            protocol::Request::Remove { window_id } => {
                match self.remove_sticky_window(window_id).await {
                    Ok(was_present) => {
                        if was_present {
                            protocol::Response::Success {
                                message: "Removed".to_string(),
                            }
                        } else {
                            protocol::Response::Success {
                                message: "Not in sticky list".to_string(),
                            }
                        }
                    }
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            }
            protocol::Request::List => match self.list_sticky_windows().await {
                Ok(windows) => protocol::Response::Data {
                    data: format!("{windows:?}"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            },
            protocol::Request::ToggleActive => match self.toggle_active_window().await {
                Ok(was_added) => {
                    if was_added {
                        protocol::Response::Success {
                            message: "Added active window to sticky".to_string(),
                        }
                    } else {
                        protocol::Response::Success {
                            message: "Removed active window from sticky".to_string(),
                        }
                    }
                }
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            },
            protocol::Request::ToggleAppid { appid } => match self.toggle_by_appid(&appid).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            },
            protocol::Request::ToggleTitle { title } => match self.toggle_by_title(&title).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            },
            protocol::Request::Stage(stage_args) => self.handle_stage_command(stage_args).await,
            protocol::Request::Unstage(unstage_args) => {
                self.handle_unstage_command(unstage_args).await
            }
        }
    }

    async fn handle_stage_command(&self, args: protocol::StageArgs) -> protocol::Response {
        if args.active {
            let active_id = match crate::system_integration::get_active_window_id().await {
                Ok(id) => id,
                Err(_) => {
                    return protocol::Response::Error {
                        message: "Failed to get active window".to_string(),
                    };
                }
            };

            if self.is_window_staged(active_id).await {
                let current_ws_id = match crate::system_integration::get_active_workspace_id().await
                {
                    Ok(id) => id,
                    Err(_) => {
                        return protocol::Response::Error {
                            message: "Failed to get active workspace ID".to_string(),
                        };
                    }
                };
                match self.unstage_active_window(current_ws_id).await {
                    Ok(()) => protocol::Response::Success {
                        message: "Unstaged active window".to_string(),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else {
                match self.stage_active_window().await {
                    Ok(()) => protocol::Response::Success {
                        message: "Staged active window".to_string(),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            }
        } else if let Some(appid) = args.appid {
            let current_ws_id = match crate::system_integration::get_active_workspace_id().await {
                Ok(id) => id,
                Err(_) => {
                    return protocol::Response::Error {
                        message: "Failed to get active workspace ID".to_string(),
                    };
                }
            };
            match self.toggle_stage_by_appid(&appid, current_ws_id).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if let Some(title) = args.title {
            let current_ws_id = match crate::system_integration::get_active_workspace_id().await {
                Ok(id) => id,
                Err(_) => {
                    return protocol::Response::Error {
                        message: "Failed to get active workspace ID".to_string(),
                    };
                }
            };
            match self.toggle_stage_by_title(&title, current_ws_id).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if args.all {
            match self.stage_all_windows().await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Staged {count} windows"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if args.list {
            match self.list_staged_windows().await {
                Ok(windows) => protocol::Response::Data {
                    data: format!("{windows:?}"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if let Some(window_id) = args.window_id {
            match self.stage_window(window_id).await {
                Ok(()) => protocol::Response::Success {
                    message: "Staged window".to_string(),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else {
            protocol::Response::Error {
                message: "Invalid stage command".to_string(),
            }
        }
    }

    async fn handle_unstage_command(&self, args: protocol::UnstageArgs) -> protocol::Response {
        let current_ws_id = match crate::system_integration::get_active_workspace_id().await {
            Ok(id) => id,
            Err(_) => {
                return protocol::Response::Error {
                    message: "Failed to get active workspace ID".to_string(),
                };
            }
        };

        if args.all {
            match self.unstage_all_windows(current_ws_id).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Unstaged {count} windows"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if args.active {
            match self.unstage_active_window(current_ws_id).await {
                Ok(()) => protocol::Response::Success {
                    message: "Unstaged active window".to_string(),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else if let Some(window_id) = args.window_id {
            match self.unstage_window(window_id, current_ws_id).await {
                Ok(()) => protocol::Response::Success {
                    message: "Unstaged window".to_string(),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        } else {
            protocol::Response::Error {
                message: "Invalid unstage command".to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auto_sticky_empty_config_does_not_sticky() {
        let business = BusinessLogic::new(crate::config::Config::default());
        let win_id = 999;
        business
            .handle_window_opened_or_changed(win_id, Some("firefox".to_string()), None)
            .await
            .unwrap();
        assert!(!business.is_window_sticky(win_id).await);
    }
}
