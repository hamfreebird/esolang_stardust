pub const SD_A: &str =   "             +             +  *      +*";            // 65
pub const SD_B: &str =   "             +             +  *       +*";
pub const SD_C: &str =   "         + + +  *  *        +*";  // 67 = 4 * 4 * 4 + 3
pub const SD_D: &str =   "         + + +  *      +*  *";  // 68 = 4 * (4 * 4 + 1)
pub const SD_E: &str =   "         +       +  * +      +*  *        + *";  // (4 * 2) * (8 + 1) - 3
pub const SD_F: &str =   "            +               +  *";
pub const SD_G: &str =   "            +               +  *      +*";  // 71
pub const SD_H: &str =   "             +              +  *";
pub const SD_I: &str =   "           + +  * +*      +*";  // 73 = 6 * 6 * 2 + 1
pub const SD_J: &str =   "";
pub const SD_K: &str =   "";
pub const SD_L: &str =   "";
pub const SD_M: &str =   "";
pub const SD_N: &str =   "";
pub const SD_O: &str =   "";
pub const SD_P: &str =   "";
pub const SD_Q: &str =   "";
pub const SD_R: &str =   "";
pub const SD_S: &str =   "";
pub const SD_T: &str =   "";
pub const SD_U: &str =   "";
pub const SD_V: &str =   "";
pub const SD_W: &str =   "";
pub const SD_X: &str =   "";
pub const SD_Y: &str =   "";
pub const SD_Z: &str =   "";
pub const SD_M_A: &str = "";  // 97
pub const SD_M_B: &str = "";
pub const SD_M_C: &str = "";
pub const SD_M_D: &str = "";
pub const SD_M_E: &str = "";
pub const SD_M_F: &str = "";
pub const SD_M_G: &str = "";
pub const SD_M_H: &str = "";
pub const SD_M_I: &str = "";  // 7×9 + 6×7
pub const SD_M_J: &str = "";
pub const SD_M_K: &str = "";
pub const SD_M_L: &str = "";
pub const SD_M_M: &str = "";
pub const SD_M_N: &str = "";
pub const SD_M_O: &str = "";
pub const SD_M_P: &str = "";
pub const SD_M_Q: &str = "";
pub const SD_M_R: &str = "";
pub const SD_M_S: &str = "";
pub const SD_M_T: &str = "";
pub const SD_M_U: &str = "";
pub const SD_M_V: &str = "";
pub const SD_M_W: &str = "";
pub const SD_M_X: &str = "";
pub const SD_M_Y: &str = "";
pub const SD_M_Z: &str = "";
pub const SD_N_1: &str = "";  // 31
pub const SD_N_2: &str = "";
pub const SD_N_3: &str = "";
pub const SD_N_4: &str = "";
pub const SD_N_5: &str = "";
pub const SD_N_6: &str = "";
pub const SD_N_7: &str = "";
pub const SD_N_8: &str = "";
pub const SD_N_9: &str = "";
pub const SD_N_0: &str = "";  // 30

/// 预定义的Hello world!
pub const HELLO_WORLD: &str = "
            +               +  *       +* +,
         +            +  *      +** +,            +* +, +,
        +* +,         +             +  * +,        +  *
              + * +,    + +,        +* +,           + * +,
             + *,        +               +  *        +*,";

pub fn spaces(n: u8) -> String {
    " ".repeat(n as usize)
}
