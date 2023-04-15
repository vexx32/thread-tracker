use std::collections::BTreeMap;

pub(crate) fn partition_into_map<TKey, TValue, F>(items: Vec<TValue>, key_function: F) -> BTreeMap<TKey, Vec<TValue>>
where
    TKey: Ord,
    F: Fn(&TValue) -> TKey,
{
    let mut map: BTreeMap<TKey, Vec<TValue>> = BTreeMap::new();

    for item in items {
        map.entry(key_function(&item)).or_default().push(item);
    }

    map
}
