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

    let family_key = if is_unknown {
        None
    } else {
        let (total, active) = if role == Role::Shard {
            (None, None)
        } else {
            (
                parsed.params_total.as_ref().map(|f| f.value),
                parsed.params_active.as_ref().map(|f| f.value),
            )
        };
        let arch = parsed.arch.as_ref().map(|f| f.value.as_str());
        Some(family_key(&name_for_key, arch, total, active))
    };

    Candidate {
        role,
        format: parsed.format.clone(),
        display_name: Field { value: name_for_key, level: name_level },
        arch: parsed.arch.clone(),
        params_total: parsed.params_total.clone(),
        params_active: parsed.params_active.clone(),
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