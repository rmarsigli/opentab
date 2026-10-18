use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ElementId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpaquePayload {
    pub format: String,
    pub data: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Extensions(pub BTreeMap<ElementId, Vec<OpaquePayload>>);
