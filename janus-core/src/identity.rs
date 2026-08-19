use crate::ev::{Field, Format, Kind, Level, Role};
use crate::filename;
use crate::parse::Parsed;

pub struct Candidate {
    pub role: Role,
    pub format: Format,
    pub display_name: Field<String>,
    pub arch: Option<Field<String>>,
    pub params_total: Option<Field<f64>>,
    pub params_active: Option<Field<f64>>,
    pub context_len: Option<Field<i64>>,
    pub kind: Field<Kind>,
    pub quant: Field<String>,
    pub quant_raw: Option<Field<String>>,
    pub subflavour: Field<String>,
    pub publisher: Field<String>,
    pub is_unknown: bool,
    pub family_key: Option<String>,
}

pub fn round1(v: f64) -> String {
    let r = ((v * 10.0).round()) / 10.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
}

/// Header/shape counts are raw; catalogue params are billions (30.5).
pub fn params_to_billions(n: f64) -> f64 {
    if n.is_finite() && n >= 1_000_000.0 {
        n / 1e9
    } else {
        n
    }
}

pub fn format_params_b(total: Option<f64>) -> String {
    match total {
        Some(t) if t.is_finite() && t > 0.0 => format!("{:.1}B", params_to_billions(t)),
        _ => "—".to_string(),
    }
}

pub fn params_identity(total: Option<f64>, active: Option<f64>) -> String {
    let t = match total {
        Some(v) => format!("t{}", round1(v)),
        None => "tunk".to_string(),
    };
    let a = match active {
        Some(v) => format!("a{}", round1(v)),
        None => "aunk".to_string(),
    };
    format!("{t}|{a}")
}

pub fn family_key(name: &str, arch: Option<&str>, total: Option<f64>, active: Option<f64>) -> String {
    format!(
        "{}|{}|{}",
        filename::slug(name),
        arch.unwrap_or("unk"),
        params_identity(total, active)
    )
}

pub fn identify(file: &str, parsed: &Parsed) -> Candidate {
    let fm = filename::stem(file);
    let role = filename::role_from_name(file);

    let has_known_facts = parsed.basename.is_some()
        || parsed.general_name.is_some()
        || parsed.arch.is_some()
        || parsed.params_total.is_some();

    let display_name = if role == Role::Shard {
        filename::shard_strip(file).unwrap_or_else(|| fm.clone())
    } else if let Some(b) = parsed.basename.as_ref() {
        b.value.clone()
    } else if let Some(n) = parsed.general_name.as_ref() {
        n.value.clone()
    } else if let Some(n) = filename::hf_snapshot_name(file) {
        n
    } else {
        filename::display_stem(file)
    };

    let quant_from_header = parsed.quant_from_header.as_ref().map(|f| (f.value.clone(), f.level));
    let quant_from_name = filename::quant_tag(&fm);
    let quant = match (&quant_from_header, quant_from_name.as_ref()) {
        (Some((q, level)), _) => Field { value: q.clone(), level: *level },
        (None, Some(q)) => Field::inferred(q.clone()),
        (None, None) => Field::inferred("unknown".to_string()),
    };
    let quant_raw = match &parsed.file_type {
        Some(ft) => Some(Field::known(ft.value.to_string())),
        None => quant_from_name.as_ref().map(|q| Field::inferred(q.clone())),
    };

    let subflavour = match filename::subflavour_tag(&fm) {
        Some(s) => Field::inferred(s.to_string()),
        None => Field::inferred("unknown".to_string()),
    };
    let publisher = match filename::publisher_token(&fm) {
        Some(p) => Field::inferred(p),
        None => Field::inferred("unknown".to_string()),
    };
    let kind = match parsed.kind.as_ref() {
        Some(k) => k.clone(),
        None => filename::kind_from_name(&fm).map(Field::inferred).unwrap_or_else(|| Field::detected(Kind::Unknown)),
    };

    let is_unknown =
        !has_known_facts && matches!(role, Role::Weights | Role::Shard) && quant_from_name.is_none();

    let (name_for_key, name_level) = if role == Role::Shard {
        (filename::display_stem(file), Level::Inferred)
    } else {
        (display_name.clone(), display_name_level(parsed))
    };

    // Shards: filename identity only. Do not treat one-shard shapes or file
    // size as known family params.
    let (params_total, params_active) = if role == Role::Shard {
        (None, None)
    } else {
        (
            parsed.params_total.as_ref().map(|f| Field {
                value: params_to_billions(f.value),
                level: f.level,
            }),
            parsed.params_active.as_ref().map(|f| Field {
                value: params_to_billions(f.value),
                level: f.level,
            }),
        )
    };

    let family_key = if is_unknown {
        None
    } else {
        let total = params_total.as_ref().map(|f| f.value);
        let active = params_active.as_ref().map(|f| f.value);
        let arch = parsed.arch.as_ref().map(|f| f.value.as_str());
        Some(family_key(&name_for_key, arch, total, active))
    };

    Candidate {
        role,
        format: parsed.format.clone(),
        display_name: Field { value: name_for_key, level: name_level },
        arch: parsed.arch.clone(),
        params_total,
        params_active,
        context_len: parsed.context_len.clone(),
        kind,
        quant,
        quant_raw,
        subflavour,
        publisher,
        is_unknown,
        family_key,
    }
}

fn display_name_level(parsed: &Parsed) -> Level {
    if parsed.basename.is_some() || parsed.general_name.is_some() {
        Level::Known
    } else {
        Level::Inferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Parsed;

    fn parsed_with_params(total: Option<f64>) -> Parsed {
        Parsed {
            format: Format::Safetensors,
            general_name: None,
            basename: None,
            finetune: None,
            arch: None,
            params_total: total.map(Field::known),
            params_active: None,
            context_len: None,
            file_type: None,
            quant_from_header: None,
            kind: None,
            parse_error: None,
        }
    }

    #[test]
    fn params_display_is_human_not_raw_count() {
        assert_eq!(format_params_b(Some(204712382976.0)), "204.7B");
        assert_eq!(format_params_b(Some(8.0)), "8.0B");
        assert_eq!(format_params_b(None), "—");
        assert_eq!(format_params_b(Some(0.0)), "—");
    }

    #[test]
    fn shard_does_not_invent_known_params() {
        let parsed = parsed_with_params(Some(204712382976.0));
        let c = identify("Kimi-K2-Instruct-00001-of-00010.safetensors", &parsed);
        assert!(c.params_total.is_none(), "filename shards must not take params from shapes or byte size");
        let key = c.family_key.expect("shard with a name is a family");
        assert!(key.contains("tunk"), "shard family key must not invent params: {key}");
    }
}