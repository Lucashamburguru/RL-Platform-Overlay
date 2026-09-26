//! Wwise boost-bank remapping. Audio and the source playback graph stay intact;
//! only the bank identity and the public play/stop event IDs change.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("Truncated sound bank")?
            .try_into()
            .unwrap(),
    ))
}

#[derive(Debug)]
struct Object {
    kind: u8,
    id: u32,
    data: Range<usize>,
}

struct Bank<'a> {
    bytes: &'a [u8],
    header: Range<usize>,
    objects: Vec<Object>,
}

impl<'a> Bank<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self, String> {
        let mut sections = BTreeMap::new();
        let mut pos = 0;
        while pos < bytes.len() {
            let tag: [u8; 4] = bytes
                .get(pos..pos + 4)
                .ok_or("Truncated sound bank section")?
                .try_into()
                .unwrap();
            let size = u32_at(bytes, pos + 4)? as usize;
            let start = pos + 8;
            let end = start
                .checked_add(size)
                .filter(|end| *end <= bytes.len())
                .ok_or("Invalid sound bank section size")?;
            if sections.insert(tag, start..end).is_some() {
                return Err("Duplicate sound bank section".into());
            }
            pos = end;
        }
        let header = sections
            .get(b"BKHD")
            .ok_or("Missing sound bank header")?
            .clone();
        if header.len() < 8 || u32_at(bytes, header.start)? != 150 {
            return Err("Sound swapping currently supports Wwise bank version 150".into());
        }
        let hirc = sections.get(b"HIRC").ok_or("Missing sound bank events")?;
        let data = &bytes[hirc.clone()];
        let count = u32_at(data, 0)? as usize;
        if count > data.len() / 9 {
            return Err("Invalid sound bank object count".into());
        }
        let mut pos = 4;
        let mut objects = Vec::new();
        let mut ids = BTreeSet::new();
        for _ in 0..count {
            let kind = *data.get(pos).ok_or("Truncated sound bank object")?;
            let size = u32_at(data, pos + 1)? as usize;
            let start = pos + 5;
            let end = start
                .checked_add(size)
                .filter(|end| *end <= data.len())
                .ok_or("Invalid sound bank object size")?;
            if size < 4 {
                return Err("Missing sound object ID".into());
            }
            let id = u32_at(data, start)?;
            if !ids.insert(id) {
                return Err("Duplicate sound object ID".into());
            }
            objects.push(Object {
                kind,
                id,
                data: hirc.start + start..hirc.start + end,
            });
            pos = end;
        }
        if pos != data.len() {
            return Err("Unexpected bytes after sound bank objects".into());
        }
        // Embedded media indexes must refer to actual bytes in the bank.
        if let Some(index) = sections.get(b"DIDX") {
            let media = sections.get(b"DATA").ok_or("Missing embedded sound data")?;
            if !index.len().is_multiple_of(12) {
                return Err("Invalid sound media index".into());
            }
            for at in (index.start..index.end).step_by(12) {
                let offset = u32_at(bytes, at + 4)? as usize;
                let size = u32_at(bytes, at + 8)? as usize;
                if offset.checked_add(size).is_none_or(|end| end > media.len()) {
                    return Err("Truncated embedded sound media".into());
                }
            }
        }
        Ok(Self {
            bytes,
            header,
            objects,
        })
    }

    fn events(&self, name: &str) -> Result<[&Object; 2], String> {
        let events = self
            .objects
            .iter()
            .filter(|o| o.kind == 4)
            .collect::<Vec<_>>();
        if events.len() != 2 {
            return Err(
                "This bank has multiple boost event pairs; sound swapping is not supported yet"
                    .into(),
            );
        }
        let stem = name
            .strip_suffix(".bnk")
            .unwrap_or(name)
            .strip_prefix("SFX_")
            .ok_or("Invalid boost bank name")?;
        let play_id = name_id(&format!("Play_{stem}_Loop"));
        let stop_id = name_id(&format!("Stop_{stem}_Loop"));
        // Verify every event's action references, even when names identify it.
        let mut roles = Vec::new();
        for event in &events {
            let data = &self.bytes[event.data.clone()];
            let count = *data.get(4).ok_or("Truncated boost event")? as usize;
            if count == 0 || data.len() != 5 + count * 4 {
                return Err("Unsupported boost event encoding".into());
            }
            let mut play = false;
            let mut stop = false;
            for i in 0..count {
                let id = u32_at(data, 5 + 4 * i)?;
                let action = self
                    .objects
                    .iter()
                    .find(|o| o.id == id && o.kind == 3)
                    .ok_or("Boost event references a missing action")?;
                let action_data = &self.bytes[action.data.clone()];
                match action_data.get(5).ok_or("Truncated boost action")? {
                    4 => play = true,
                    1 => stop = true,
                    _ => (),
                }
            }
            roles.push((play, stop));
        }
        if let (Some(play), Some(stop)) = (
            events.iter().find(|o| o.id == play_id),
            events.iter().find(|o| o.id == stop_id),
        ) {
            return Ok([play, stop]);
        }
        let starts = roles
            .iter()
            .enumerate()
            .filter(|(_, (play, stop))| *play && !*stop)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let stops = roles
            .iter()
            .enumerate()
            .filter(|(_, (_, stop))| *stop)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if starts.len() == 1 && stops.len() == 1 {
            return Ok([events[starts[0]], events[stops[0]]]);
        }
        Err("Could not identify this bank's play and stop events unambiguously".into())
    }
}

fn name_id(name: &str) -> u32 {
    name.bytes().fold(2_166_136_261, |hash, byte| {
        hash.wrapping_mul(16_777_619) ^ u32::from(byte.to_ascii_lowercase())
    })
}

pub fn valid_bank_name(name: &str) -> bool {
    name.starts_with("SFX_Boost")
        && name.ends_with(".bnk")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        && !name.contains("..")
}

pub fn validate(bytes: &[u8], name: &str) -> Result<(), String> {
    if !valid_bank_name(name) {
        return Err("Invalid boost sound bank name".into());
    }
    let bank = Bank::read(bytes)?;
    if u32_at(bytes, bank.header.start + 4)? != name_id(name.trim_end_matches(".bnk")) {
        return Err("Sound bank identity does not match its filename".into());
    }
    bank.events(name)?;
    Ok(())
}

pub fn generate(
    source: &[u8],
    target: &[u8],
    source_name: &str,
    target_name: &str,
) -> Result<Vec<u8>, String> {
    validate(source, source_name)?;
    validate(target, target_name)?;
    let donor = Bank::read(source)?;
    let target = Bank::read(target)?;
    let source_events = donor.events(source_name)?;
    let target_events = target.events(target_name)?;
    let mut out = source.to_vec();
    out[donor.header.start + 4..donor.header.start + 8]
        .copy_from_slice(&target.bytes[target.header.start + 4..target.header.start + 8]);
    for (source_event, target_event) in source_events.into_iter().zip(target_events) {
        if donor
            .objects
            .iter()
            .any(|o| o.kind != 4 && o.id == target_event.id)
        {
            return Err("Target event ID conflicts with a source sound object".into());
        }
        out[source_event.data.start..source_event.data.start + 4]
            .copy_from_slice(&target_event.id.to_le_bytes());
    }
    validate(&out, target_name)?;
    Ok(out)
}

pub fn display_name(name: &str) -> String {
    if name == "SFX_Boost_Alpha.bnk" {
        return "Alpha / Dev Boost (Gold Rush)".into();
    }
    name.trim_end_matches(".bnk")
        .trim_start_matches("SFX_Boost_")
        .replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        out.extend(tag);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
    }
    fn fixture(name: &str, media: &[u8]) -> Vec<u8> {
        let stem = name.trim_start_matches("SFX_").trim_end_matches(".bnk");
        let mut out = Vec::new();
        let header = [150, name_id(name.trim_end_matches(".bnk"))]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        section(&mut out, b"BKHD", &header);
        let index = [123, 0, media.len() as u32]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        section(&mut out, b"DIDX", &index);
        section(&mut out, b"DATA", media);
        let mut hirc = 4u32.to_le_bytes().to_vec();
        for (kind, id, payload) in [
            (3, 10, vec![3, 4]),
            (3, 11, vec![3, 1]),
            (
                4,
                name_id(&format!("Play_{stem}_Loop")),
                vec![1, 10, 0, 0, 0],
            ),
            (
                4,
                name_id(&format!("Stop_{stem}_Loop")),
                vec![1, 11, 0, 0, 0],
            ),
        ] {
            hirc.push(kind);
            hirc.extend((4 + payload.len() as u32).to_le_bytes());
            hirc.extend(id.to_le_bytes());
            hirc.extend(payload);
        }
        section(&mut out, b"HIRC", &hirc);
        out
    }

    #[test]
    fn maps_only_bank_and_public_events_preserving_playback_graph_and_media() {
        let source = fixture("SFX_Boost_Alpha.bnk", b"alpha media bytes");
        let target = fixture("SFX_Boost_Standard.bnk", b"original audio");
        let out = generate(
            &source,
            &target,
            "SFX_Boost_Alpha.bnk",
            "SFX_Boost_Standard.bnk",
        )
        .unwrap();
        let source_bank = Bank::read(&source).unwrap();
        let target_bank = Bank::read(&target).unwrap();
        let result_bank = Bank::read(&out).unwrap();
        let source_events = source_bank.events("SFX_Boost_Alpha.bnk").unwrap();
        let target_events = target_bank.events("SFX_Boost_Standard.bnk").unwrap();
        let result_events = result_bank.events("SFX_Boost_Standard.bnk").unwrap();
        assert_eq!(result_events.map(|o| o.id), target_events.map(|o| o.id));
        assert_eq!(source.len(), out.len());
        for i in 0..source.len() {
            let is_identity = (source_bank.header.start + 4..source_bank.header.start + 8)
                .contains(&i)
                || source_events
                    .iter()
                    .any(|o| (o.data.start..o.data.start + 4).contains(&i));
            if !is_identity {
                assert_eq!(source[i], out[i], "modified byte {i}");
            }
        }
    }

    #[test]
    fn refuses_wrong_identity_truncation_and_unknown_version() {
        let mut bytes = fixture("SFX_Boost_Alpha.bnk", b"audio");
        assert!(validate(&bytes, "SFX_Boost_Standard.bnk").is_err());
        for len in [0, 7, bytes.len() - 1] {
            assert!(validate(&bytes[..len], "SFX_Boost_Alpha.bnk").is_err());
        }
        bytes[8..12].copy_from_slice(&151u32.to_le_bytes());
        assert!(validate(&bytes, "SFX_Boost_Alpha.bnk").is_err());
        assert!(!valid_bank_name("../SFX_Boost_Alpha.bnk"));
        assert!(!valid_bank_name("SFX_Boost_../../other.bnk"));
    }

    #[test]
    fn refuses_ambiguous_event_roles_instead_of_guessing_order() {
        let mut bytes = fixture("SFX_Boost_Mixed.bnk", b"audio");
        let bank = Bank::read(&bytes).unwrap();
        let events = bank
            .events("SFX_Boost_Mixed.bnk")
            .unwrap()
            .map(|e| e.data.start);
        let stop_action = bank
            .objects
            .iter()
            .find(|o| o.kind == 3 && o.id == 11)
            .unwrap()
            .data
            .start;
        bytes[events[0]..events[0] + 4].copy_from_slice(&99u32.to_le_bytes());
        bytes[events[1]..events[1] + 4].copy_from_slice(&100u32.to_le_bytes());
        bytes[stop_action + 5] = 4;
        assert!(validate(&bytes, "SFX_Boost_Mixed.bnk").is_err());
    }
}
