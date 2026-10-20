use opentab_model::InstrumentFamily;

const FAMILY_NAMES: [&str; 17] = [
    "piano",
    "chromatic_percussion",
    "organ",
    "guitar",
    "bass",
    "strings",
    "ensemble",
    "brass",
    "reed",
    "pipe",
    "synth_lead",
    "synth_pad",
    "synth_effects",
    "ethnic",
    "percussive",
    "sound_effects",
    "drums",
];

pub fn parse_family(text: &str) -> Result<InstrumentFamily, String> {
    let normal = text.to_ascii_lowercase().replace('-', "_");
    serde_json::from_value(serde_json::Value::String(normal)).map_err(|_| {
        format!(
            "{text:?} is not an instrument family; use one of {}",
            FAMILY_NAMES.join(", ")
        )
    })
}

pub fn family_name(family: InstrumentFamily) -> String {
    match serde_json::to_value(family) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_parses_and_names_itself() {
        for name in FAMILY_NAMES {
            let family = parse_family(name).expect(name);
            assert_eq!(family_name(family), name);
        }
        assert_eq!(parse_family("Synth-Lead"), Ok(InstrumentFamily::SynthLead));
        assert!(parse_family("banjo").is_err());
    }
}
