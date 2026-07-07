use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    Add { window_id: u64 },
    Remove { window_id: u64 },
    List,
    ToggleActive,
    ToggleAppid { appid: String },
    ToggleTitle { title: String },
    StageList,
    Stage { window_id: u64 },
    Unstage { window_id: u64 },
    StageToggleActive,
    StageToggleAppid { appid: String },
    StageToggleTitle { title: String },
    StageAll,
    UnstageAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Success { message: String },
    Error { message: String },
    Data { data: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_roundtrip_add() {
        let req = Request::Add { window_id: 42 };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"command":"add","window_id":42}"#);
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, req);
    }

    #[test]
    fn test_request_roundtrip_toggle_active() {
        let req = Request::ToggleActive;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"command":"toggle_active"}"#);
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, req);
    }

    #[test]
    fn test_request_roundtrip_stage_toggle_appid() {
        let req = Request::StageToggleAppid {
            appid: "firefox".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"command":"stage_toggle_appid","appid":"firefox"}"#
        );
        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, req);
    }

    #[test]
    fn test_response_roundtrip_success() {
        let resp = Response::Success {
            message: "Done".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"success","message":"Done"}"#);
        let deserialized: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, resp);
    }

    #[test]
    fn test_response_roundtrip_error() {
        let resp = Response::Error {
            message: "Something broke".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"error","message":"Something broke"}"#);
        let deserialized: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, resp);
    }

    #[test]
    fn test_response_roundtrip_data() {
        let resp = Response::Data {
            data: "[1, 2, 3]".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"data","data":"[1, 2, 3]"}"#);
        let deserialized: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, resp);
    }
}
