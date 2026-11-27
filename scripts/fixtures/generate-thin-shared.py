import pathlib
import sys
import xml.etree.ElementTree as ET

root = ET.Element(
    "superblock", uuid="", time="7", transaction="5",
    data_block_size="128", nr_data_blocks="4096",
)
for device_id in range(1, 5):
    device = ET.SubElement(
        root, "device", dev_id=str(device_id), mapped_blocks="512",
        transaction="4", creation_time="3", snap_time="6",
    )
    ET.SubElement(
        device, "range_mapping", origin_begin="0", data_begin="0",
        length="512", time="5",
    )
ET.ElementTree(root).write(pathlib.Path(sys.argv[1]), encoding="utf-8")
