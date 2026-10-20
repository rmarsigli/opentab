use std::ffi::OsString;
use std::path::PathBuf;

pub const DATA_DIR_VARIABLE: &str = "OPENTAB_DATA_DIR";
pub const DATA_DIR_NAME: &str = "opentab";

pub(crate) fn data_dir_from(
    variable: Option<OsString>,
    platform: Option<PathBuf>,
) -> Option<PathBuf> {
    match variable {
        Some(value) if !value.is_empty() => Some(PathBuf::from(value)),
        _ => platform.map(|folder| folder.join(DATA_DIR_NAME)),
    }
}

pub fn default_data_dir() -> Option<PathBuf> {
    data_dir_from(std::env::var_os(DATA_DIR_VARIABLE), dirs::data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_wins_over_the_platform_folder() {
        assert_eq!(
            data_dir_from(
                Some(OsString::from("/tmp/x")),
                Some(PathBuf::from("/home/a/.local/share"))
            ),
            Some(PathBuf::from("/tmp/x"))
        );
    }

    #[test]
    fn an_empty_or_missing_variable_uses_the_platform_folder() {
        let platform = Some(PathBuf::from("/home/a/.local/share"));
        let expected = Some(PathBuf::from("/home/a/.local/share/opentab"));
        assert_eq!(
            data_dir_from(Some(OsString::new()), platform.clone()),
            expected
        );
        assert_eq!(data_dir_from(None, platform), expected);
        assert_eq!(data_dir_from(None, None), None);
    }
}
