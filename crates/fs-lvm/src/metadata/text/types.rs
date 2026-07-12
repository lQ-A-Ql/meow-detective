pub(crate) type Params = Vec<(String, String)>;
pub(crate) type NamedParams = (String, Params);

#[derive(Debug)]
pub(crate) struct SegmentRaw {
    pub(crate) name: String,
    pub(crate) params: Params,
}

pub(super) struct LvSectionRaw {
    pub(super) name: String,
    pub(super) params: Params,
    pub(super) segments: Vec<SegmentRaw>,
}

pub(super) struct ParsedSection {
    pub(super) name: String,
    pub(super) params: Params,
    pub(super) pv_sections: Vec<NamedParams>,
    pub(super) lv_sections: Vec<LvSectionRaw>,
}
