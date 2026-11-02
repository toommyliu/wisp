use super::{Destination, InputError, Request, text};

pub(super) fn parse(request: &Request) -> Result<Option<Destination>, InputError> {
    let destination = if let Request::Join {
        target,
        random_room,
        ..
    } = request
    {
        let map = text::trim(target);
        Some(if let Some(index) = room_separator(map) {
            let token = &map[index + 1..];
            let fixed = token
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| token.trim_start_matches('0').parse::<u64>().ok())
                .flatten()
                .filter(|room| (1..=9_007_199_254_740_991).contains(room));
            let room = match fixed {
                Some(room) => room,
                None => u64::from(
                    random_room
                        .filter(|room| (10000..=99999).contains(room))
                        .ok_or(InputError::RandomRoomRequired)?,
                ),
            };
            Destination {
                map: if fixed.is_some() {
                    map.to_owned()
                } else {
                    format!("{}-{room}", &map[..index])
                },
                name: map[..index].to_owned(),
                exact_room: (fixed.is_none() || (1001..=99999).contains(&room)).then_some(room),
            }
        } else {
            Destination {
                map: map.to_owned(),
                name: map.to_owned(),
                exact_room: None,
            }
        })
    } else {
        None
    };

    Ok(destination)
}

fn room_separator(map: &str) -> Option<usize> {
    map.find('-')
        .filter(|index| *index > 0 && *index < map.len() - 1)
}

pub(crate) fn map_name(target: &str) -> &str {
    let map = text::trim(target);
    room_separator(map).map_or(map, |index| &map[..index])
}
