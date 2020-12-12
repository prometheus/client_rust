pub trait LabelSet: std::hash::Hash + Eq {}

impl LabelSet for Vec<(String, String)> {}
