use crate::{
    update::{Channel, BUILD_CHANNEL_PATH},
    util,
};
use std::path::Path;

const PREFERENCE: &str = ".installer/update-channel";

fn read_channel(root: &Path, file: &str) -> Option<Channel> {
    let text = std::fs::read_to_string(root.join(file)).ok()?;
    Channel::parse(text.trim())
}

pub fn preferred(root: &Path) -> Channel {
    read_channel(root, PREFERENCE).unwrap_or(Channel::Stable)
}

pub fn set_preferred(root: &Path, channel: Channel) -> util::Result<()> {
    util::write_file(
        &root.join(PREFERENCE),
        format!("{}\n", channel.as_str()).as_bytes(),
    )
}

pub fn installed(root: &Path) -> Option<Channel> {
    read_channel(root, BUILD_CHANNEL_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_defaults_to_stable_and_does_not_change_the_installed_channel() {
        let root = std::env::temp_dir().join(format!("infamous-channel-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        assert_eq!(preferred(&root), Channel::Stable);
        assert_eq!(installed(&root), None);

        util::write_file(&root.join(BUILD_CHANNEL_PATH), b"bleeding-edge\n").unwrap();
        assert_eq!(preferred(&root), Channel::Stable);
        assert_eq!(installed(&root), Some(Channel::BleedingEdge));

        set_preferred(&root, Channel::BleedingEdge).unwrap();
        assert_eq!(preferred(&root), Channel::BleedingEdge);
        set_preferred(&root, Channel::Stable).unwrap();
        assert_eq!(preferred(&root), Channel::Stable);
        assert_eq!(installed(&root), Some(Channel::BleedingEdge));
        std::fs::remove_dir_all(root).unwrap();
    }
}
