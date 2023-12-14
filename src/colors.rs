#![allow(unused)]
use gtk::gdk;

pub const BLUE_1: gdk::RGBA = gdk::RGBA::new(0.6, 0.756_862_76, 0.945_098_04, 1.0); // #99c1f1
pub const BLUE_2: gdk::RGBA = gdk::RGBA::new(0.384_313_73, 0.627_451, 0.917_647_06, 1.0); // #62a0ea
pub const BLUE_3: gdk::RGBA = gdk::RGBA::new(0.207_843_14, 0.517_647_1, 0.894_117_65, 1.0); // #3584e4
pub const BLUE_4: gdk::RGBA = gdk::RGBA::new(0.109_803_92, 0.443_137_26, 0.847_058_83, 1.0); // #1c71d8
pub const BLUE_5: gdk::RGBA = gdk::RGBA::new(0.101_960_786, 0.372_549_03, 0.705_882_4, 1.0); // #1a5fb4

pub const GREEN_1: gdk::RGBA = gdk::RGBA::new(0.560_784_34, 0.941_176_5, 0.643_137_3, 1.0); // #8ff0a4
pub const GREEN_2: gdk::RGBA = gdk::RGBA::new(0.341_176_48, 0.890_196_1, 0.537_254_9, 1.0); // #57e389
pub const GREEN_3: gdk::RGBA = gdk::RGBA::new(0.2, 0.819_607_85, 0.478_431_37, 1.0); // #33d17a
pub const GREEN_4: gdk::RGBA = gdk::RGBA::new(0.180_392_16, 0.760_784_3, 0.494_117_65, 1.0); // #2ec27e
pub const GREEN_5: gdk::RGBA = gdk::RGBA::new(0.149_019_61, 0.635_294_14, 0.411_764_7, 1.0); // #26a269

pub const YELLOW_1: gdk::RGBA = gdk::RGBA::new(0.976_470_6, 0.941_176_5, 0.419_607_85, 1.0); // #f9f06b
pub const YELLOW_2: gdk::RGBA = gdk::RGBA::new(0.972_549, 0.894_117_65, 0.360_784_32, 1.0); // #f8e45c
pub const YELLOW_3: gdk::RGBA = gdk::RGBA::new(0.964_705_9, 0.827_451, 0.176_470_6, 1.0); // #f6d32d
pub const YELLOW_4: gdk::RGBA = gdk::RGBA::new(0.960_784_3, 0.760_784_3, 0.066_666_67, 1.0); // #f5c211
pub const YELLOW_5: gdk::RGBA = gdk::RGBA::new(0.898_039_2, 0.647_058_84, 0.039_215_688, 1.0); // #e5a50a

pub const ORANGE_1: gdk::RGBA = gdk::RGBA::new(1.0, 0.745_098_05, 0.435_294_12, 1.0); // #ffbe6f
pub const ORANGE_2: gdk::RGBA = gdk::RGBA::new(1.0, 0.639_215_7, 0.282_352_95, 1.0); // #ffa348
pub const ORANGE_3: gdk::RGBA = gdk::RGBA::new(1.0, 0.470_588_24, 0.0, 1.0); // #ff7800
pub const ORANGE_4: gdk::RGBA = gdk::RGBA::new(0.901_960_8, 0.380_392_16, 0.0, 1.0); // #e66100
pub const ORANGE_5: gdk::RGBA = gdk::RGBA::new(0.776_470_6, 0.274_509_82, 0.0, 1.0); // #c64600

pub const RED_1: gdk::RGBA = gdk::RGBA::new(0.964_705_9, 0.380_392_16, 0.317_647_07, 1.0); // #f66151
pub const RED_2: gdk::RGBA = gdk::RGBA::new(0.929_411_77, 0.2, 0.231_372_55, 1.0); // #ed333b
pub const RED_3: gdk::RGBA = gdk::RGBA::new(0.878_431_4, 0.105_882_354, 0.141_176_48, 1.0); // #e01b24
pub const RED_4: gdk::RGBA = gdk::RGBA::new(0.752_941_2, 0.109_803_92, 0.156_862_75, 1.0); // #c01c28
pub const RED_5: gdk::RGBA = gdk::RGBA::new(0.647_058_84, 0.113_725_49, 0.176_470_6, 1.0); // #a51d2d

pub const PURPLE_1: gdk::RGBA = gdk::RGBA::new(0.862_745_1, 0.541_176_5, 0.866_666_7, 1.0); // #dc8add
pub const PURPLE_2: gdk::RGBA = gdk::RGBA::new(0.752_941_2, 0.380_392_16, 0.796_078_44, 1.0); // #c061cb
pub const PURPLE_3: gdk::RGBA = gdk::RGBA::new(0.568_627_5, 0.254_901_98, 0.674_509_8, 1.0); // #9141ac
pub const PURPLE_4: gdk::RGBA = gdk::RGBA::new(0.505_882_4, 0.239_215_69, 0.611_764_7, 1.0); // #813d9c
pub const PURPLE_5: gdk::RGBA = gdk::RGBA::new(0.380_392_16, 0.207_843_14, 0.513_725_5, 1.0); // #613583

pub const BROWN_1: gdk::RGBA = gdk::RGBA::new(0.803_921_6, 0.670_588_25, 0.560_784_34, 1.0); // #cdab8f
pub const BROWN_2: gdk::RGBA = gdk::RGBA::new(0.709_803_94, 0.513_725_5, 0.352_941_2, 1.0); // #b5835a
pub const BROWN_3: gdk::RGBA = gdk::RGBA::new(0.596_078_46, 0.415_686_28, 0.266_666_68, 1.0); // #986a44
pub const BROWN_4: gdk::RGBA = gdk::RGBA::new(0.525_490_2, 0.368_627_46, 0.235_294_12, 1.0); // #865e3c
pub const BROWN_5: gdk::RGBA = gdk::RGBA::new(0.388_235_3, 0.270_588_25, 0.172_549_02, 1.0); // #63452c

pub const LIGHT_1: gdk::RGBA = gdk::RGBA::WHITE; // #ffffff
pub const LIGHT_2: gdk::RGBA = gdk::RGBA::new(0.964_705_9, 0.960_784_3, 0.956_862_75, 1.0); // #f6f5f4
pub const LIGHT_3: gdk::RGBA = gdk::RGBA::new(0.870_588_24, 0.866_666_7, 0.854_901_97, 1.0); // #deddda
pub const LIGHT_4: gdk::RGBA = gdk::RGBA::new(0.752_941_2, 0.749_019_6, 0.737_254_9, 1.0); // #c0bfbc
pub const LIGHT_5: gdk::RGBA = gdk::RGBA::new(0.603_921_6, 0.6, 0.588_235_3, 1.0); // #9a9996

pub const DARK_1: gdk::RGBA = gdk::RGBA::new(0.466_666_67, 0.462_745_1, 0.482_352_94, 1.0); // #77767b
pub const DARK_2: gdk::RGBA = gdk::RGBA::new(0.368_627_46, 0.360_784_32, 0.392_156_87, 1.0); // #5e5c64
pub const DARK_3: gdk::RGBA = gdk::RGBA::new(0.239_215_69, 0.219_607_84, 0.274_509_82, 1.0); // #3d3846
pub const DARK_4: gdk::RGBA = gdk::RGBA::new(0.141_176_48, 0.121_568_63, 0.192_156_87, 1.0); // #241f31
pub const DARK_5: gdk::RGBA = gdk::RGBA::BLACK; // #000000
