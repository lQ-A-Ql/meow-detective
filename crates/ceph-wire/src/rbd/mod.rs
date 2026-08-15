mod metadata;
mod striping;

pub const RBD_HEAD_SNAP_HEX: &str = "fffffffffffffffe";

pub use metadata::{
    decode_rbd_data_pool_id, decode_rbd_features, decode_rbd_id, decode_rbd_name,
    decode_rbd_object_prefix, decode_rbd_order, decode_rbd_size, decode_rbd_string,
    decode_rbd_stripe_count, decode_rbd_stripe_unit, RbdImageMetadata,
};
pub use striping::{format_rbd_data_object_name, RbdHeadImageLayout, RbdReadPlan};
