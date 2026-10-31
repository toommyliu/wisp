use lucent_protocol::combat_profile::CombatProfile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsumableItem {
    pub item_id: i64,
    pub name: String,
    pub category: String,
    pub link: String,
}

pub fn consumable_warning(name: &str, item: Option<&ConsumableItem>) -> Option<String> {
    let Some(item) = item else {
        return Some(format!("{name} was not found."));
    };
    let link = item.link.trim_matches(super::js_space).to_lowercase();
    (item.category != "Item" || matches!(link.as_str(), "" | "none" | "elixir" | "tonic"))
        .then(|| format!("{} cannot be equipped.", item.name))
}

pub fn class_warning(
    profile: &CombatProfile,
    equipped_item: Option<&str>,
    class_name: &str,
) -> Option<String> {
    let classes = profile.class_names.as_deref().unwrap_or_default();
    if classes.is_empty()
        || classes.iter().any(|expected| {
            expected.to_lowercase() == equipped_item.unwrap_or_default().to_lowercase()
                || expected.to_lowercase() == class_name.to_lowercase()
        })
    {
        return None;
    }
    let equipped = equipped_item
        .filter(|name| !name.is_empty())
        .unwrap_or(class_name);
    if equipped.is_empty() {
        return None;
    }
    let list = match classes {
        [only] => only.clone(),
        [first, second] => format!("{first} or {second}"),
        _ => format!(
            "{}, or {}",
            classes[..classes.len() - 1].join(", "),
            classes.last()?
        ),
    };
    Some(format!(
        "This profile is for {list}, but {equipped} is equipped."
    ))
}
