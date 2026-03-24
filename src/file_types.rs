use crate::types::FileTypeFilter;
use crate::search::file_type_category;

/// Map a file extension to an RGB color for treemap rendering.
pub fn color_for_extension(ext: &str) -> [u8; 3] {
    match file_type_category(ext) {
        FileTypeFilter::Images    => [76,  175, 80],   // green
        FileTypeFilter::Videos    => [33,  150, 243],  // blue
        FileTypeFilter::Audio     => [156, 39,  176],  // purple
        FileTypeFilter::Documents => [255, 152, 0],    // orange
        FileTypeFilter::Code      => [0,   188, 212],  // cyan
        FileTypeFilter::Archives  => [244, 67,  54],   // red
        FileTypeFilter::Other     => [120, 120, 140],  // gray
        FileTypeFilter::All       => [120, 120, 140],
    }
}
