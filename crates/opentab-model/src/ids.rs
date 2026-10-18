use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementId(pub u64);

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub ElementId);

        impl From<ElementId> for $name {
            fn from(id: ElementId) -> Self {
                Self(id)
            }
        }

        impl From<$name> for ElementId {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

typed_id!(TrackId);
typed_id!(StaffId);
typed_id!(MasterBarId);
typed_id!(BarId);
typed_id!(VoiceId);
typed_id!(BeatId);
typed_id!(NoteId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<ElementId> {
        if self.next == u64::MAX {
            return None;
        }
        let id = ElementId(self.next);
        self.next += 1;
        Some(id)
    }

    pub fn peek(&self) -> ElementId {
        ElementId(self.next)
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self { next: 1 }
    }
}
