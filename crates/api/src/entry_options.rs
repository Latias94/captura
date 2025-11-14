use captura_storage::entity::entry;
use sea_orm::Set;

/// Flags that can be updated on an entry.
pub(crate) struct EntryUpdateFlags {
    pub is_read: Option<bool>,
    pub is_starred: Option<bool>,
}

/// Apply flag updates to an entry ActiveModel.
pub(crate) fn apply_entry_flags(am: &mut entry::ActiveModel, flags: EntryUpdateFlags) {
    if let Some(v) = flags.is_read {
        am.is_read = Set(v);
    }
    if let Some(v) = flags.is_starred {
        am.is_starred = Set(v);
    }
}
