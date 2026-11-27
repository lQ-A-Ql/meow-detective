import pathlib
import sys
import xml.etree.ElementTree as ET

root = ET.Element(
    "superblock", uuid="", time="7", transaction="5",
    data_block_size="128", nr_data_blocks="4096",
)
for device_id in range(1, 4):
    device = ET.SubElement(
        root, "device", dev_id=str(device_id), mapped_blocks="1024",
        transaction="4", creation_time="3", snap_time="6",
    )
    for index in range(1024):
        ET.SubElement(
            device, "single_mapping", origin_block=str(index * 2),
            data_block=str(index + (device_id - 1) * 1024), time="5",
        )
ET.ElementTree(root).write(pathlib.Path(sys.argv[1]), encoding="utf-8")
