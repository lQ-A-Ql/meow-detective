mod device_facts;
mod extraction;
mod model;
mod package_metadata;
mod persistence;
mod query;
mod source_files;

pub(crate) use extraction::run_android_source_analysis;
pub(crate) use query::{
    get_android_device_info, get_android_package_summary, hydrate_package_presentations,
};
