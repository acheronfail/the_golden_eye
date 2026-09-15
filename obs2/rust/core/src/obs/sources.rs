use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(rename = "ObsSource")]
pub struct Source {
    pub name: String,
    pub id: String,
}

pub fn collect_sources() -> Vec<Source> {
    crate::obs::source_names().into_iter().map(|(name, id)| Source { name, id }).collect()
}
