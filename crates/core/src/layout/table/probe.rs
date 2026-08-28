use super::types::{TableProbePolicy, TableSettings};

pub(crate) fn should_probe_tables(settings: &TableSettings) -> bool {
    match settings.probe_policy {
        TableProbePolicy::Never => false,
        TableProbePolicy::Always => true,
        TableProbePolicy::Auto => !uses_text_strategy(settings),
    }
}

fn uses_text_strategy(settings: &TableSettings) -> bool {
    settings.vertical_strategy.uses_text() || settings.horizontal_strategy.uses_text()
}
