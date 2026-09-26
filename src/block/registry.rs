//! Allocation-free mapping from compact block values to family definitions.

use super::direction;
use super::fluids;
use super::lights;
use super::ores;
use super::plants;
use super::terrain;
use super::utility;
use crate::block::definition::BlockDefinition;
use crate::block::id::Id;

const fn build_definition_table() -> [&'static dyn BlockDefinition; 256] {
    let mut table = [&utility::DEFINITION as &dyn BlockDefinition; 256];
    table[0] = &terrain::DEFINITION;
    table[1] = &terrain::DEFINITION;
    table[2] = &terrain::DEFINITION;
    table[3] = &terrain::DEFINITION;
    table[4] = &terrain::DEFINITION;
    table[5] = &terrain::DEFINITION;
    table[7] = &terrain::DEFINITION;
    table[12] = &terrain::DEFINITION;
    table[13] = &terrain::DEFINITION;
    table[19] = &terrain::DEFINITION;
    table[20] = &terrain::DEFINITION;
    table[24] = &terrain::DEFINITION;
    table[25] = &terrain::DEFINITION;
    table[35] = &terrain::DEFINITION;
    table[41] = &terrain::DEFINITION;
    table[42] = &terrain::DEFINITION;
    table[43] = &terrain::DEFINITION;
    table[44] = &terrain::DEFINITION;
    table[45] = &terrain::DEFINITION;
    table[46] = &terrain::DEFINITION;
    table[47] = &terrain::DEFINITION;
    table[48] = &terrain::DEFINITION;
    table[49] = &terrain::DEFINITION;
    table[52] = &terrain::DEFINITION;
    table[53] = &terrain::DEFINITION;
    table[67] = &terrain::DEFINITION;
    table[57] = &terrain::DEFINITION;
    table[58] = &terrain::DEFINITION;
    table[60] = &terrain::DEFINITION;
    table[78] = &terrain::DEFINITION;
    table[79] = &terrain::DEFINITION;
    table[80] = &terrain::DEFINITION;
    table[82] = &terrain::DEFINITION;
    table[84] = &terrain::DEFINITION;
    table[85] = &terrain::DEFINITION;
    table[88] = &terrain::DEFINITION;
    table[92] = &terrain::DEFINITION;
    table[95] = &terrain::DEFINITION;
    table[96] = &terrain::DEFINITION;
    table[87] = &terrain::DEFINITION;
    table[14] = &ores::DEFINITION;
    table[15] = &ores::DEFINITION;
    table[16] = &ores::DEFINITION;
    table[21] = &ores::DEFINITION;
    table[22] = &ores::DEFINITION;
    table[56] = &ores::DEFINITION;
    table[73] = &ores::DEFINITION;
    table[74] = &ores::DEFINITION;
    table[6] = &plants::DEFINITION;
    table[17] = &plants::DEFINITION;
    table[18] = &plants::DEFINITION;
    table[31] = &plants::DEFINITION;
    table[32] = &plants::DEFINITION;
    table[37] = &plants::DEFINITION;
    table[38] = &plants::DEFINITION;
    table[39] = &plants::DEFINITION;
    table[40] = &plants::DEFINITION;
    table[59] = &plants::DEFINITION;
    table[81] = &plants::DEFINITION;
    table[83] = &plants::DEFINITION;
    table[200] = &plants::DEFINITION;
    table[201] = &plants::DEFINITION;
    table[202] = &plants::DEFINITION;
    table[203] = &plants::DEFINITION;
    table[217] = &plants::DEFINITION;
    table[218] = &plants::DEFINITION;
    table[208] = &plants::DEFINITION;
    table[8] = &fluids::DEFINITION;
    table[9] = &fluids::DEFINITION;
    table[10] = &fluids::DEFINITION;
    table[11] = &fluids::DEFINITION;
    table[50] = &lights::DEFINITION;
    table[51] = &lights::DEFINITION;
    table[75] = &lights::DEFINITION;
    table[76] = &lights::DEFINITION;
    table[89] = &lights::DEFINITION;
    table[91] = &lights::DEFINITION;
    table[204] = &lights::DEFINITION;
    table[205] = &lights::DEFINITION;
    table[206] = &lights::DEFINITION;
    table[207] = &lights::DEFINITION;
    table[54] = &direction::DEFINITION;
    table[223] = &direction::DEFINITION;
    table[224] = &direction::DEFINITION;
    table[225] = &direction::DEFINITION;
    table[226] = &direction::DEFINITION;
    table[65] = &direction::DEFINITION;
    table[227] = &direction::DEFINITION;
    table[228] = &direction::DEFINITION;
    table[229] = &direction::DEFINITION;
    table[230] = &direction::DEFINITION;
    table[61] = &direction::DEFINITION;
    table[62] = &direction::DEFINITION;
    table[209] = &direction::DEFINITION;
    table[210] = &direction::DEFINITION;
    table[211] = &direction::DEFINITION;
    table[212] = &direction::DEFINITION;
    table[213] = &direction::DEFINITION;
    table[214] = &direction::DEFINITION;
    table[215] = &direction::DEFINITION;
    table[216] = &direction::DEFINITION;
    table[86] = &direction::DEFINITION;
    table[219] = &direction::DEFINITION;
    table[220] = &direction::DEFINITION;
    table[221] = &direction::DEFINITION;
    table[222] = &direction::DEFINITION;
    table[26] = &utility::DEFINITION;
    table[30] = &utility::DEFINITION;
    table[28] = &utility::DEFINITION;
    table[23] = &utility::DEFINITION;
    table[71] = &utility::DEFINITION;
    table[69] = &utility::DEFINITION;
    table[36] = &utility::DEFINITION;
    table[90] = &utility::DEFINITION;
    table[33] = &utility::DEFINITION;
    table[34] = &utility::DEFINITION;
    table[27] = &utility::DEFINITION;
    table[94] = &utility::DEFINITION;
    table[66] = &utility::DEFINITION;
    table[55] = &utility::DEFINITION;
    table[93] = &utility::DEFINITION;
    table[63] = &utility::DEFINITION;
    table[29] = &utility::DEFINITION;
    table[77] = &utility::DEFINITION;
    table[70] = &utility::DEFINITION;
    table[68] = &utility::DEFINITION;
    table[64] = &utility::DEFINITION;
    table[72] = &utility::DEFINITION;
    table
}

static DEFINITIONS: [&dyn BlockDefinition; 256] = build_definition_table();

pub(crate) fn definition(id: Id) -> &'static dyn BlockDefinition {
    if matches!(id, Id::Unknown(_)) {
        &utility::DEFINITION
    } else {
        DEFINITIONS[id.as_u8() as usize]
    }
}
