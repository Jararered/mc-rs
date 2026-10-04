//! Allocation-free mapping from compact block values to family definitions.

use super::direction;
use super::fluids;
use super::lights;
use super::ores;
use super::plants;
use super::terrain;
use super::utility;
use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;

const fn build_definition_table() -> [&'static dyn BlockDefinition; 256] {
    let mut table = [&utility::DEFINITION as &dyn BlockDefinition; 256];
    table[0] = &terrain::TERRAIN_DEFINITION;
    table[1] = &terrain::TERRAIN_DEFINITION;
    table[2] = &terrain::TERRAIN_DEFINITION;
    table[3] = &terrain::TERRAIN_DEFINITION;
    table[4] = &terrain::TERRAIN_DEFINITION;
    table[5] = &terrain::TERRAIN_DEFINITION;
    table[7] = &terrain::TERRAIN_DEFINITION;
    table[12] = &terrain::TERRAIN_DEFINITION;
    table[13] = &terrain::TERRAIN_DEFINITION;
    table[19] = &terrain::TERRAIN_DEFINITION;
    table[20] = &terrain::TERRAIN_DEFINITION;
    table[24] = &terrain::TERRAIN_DEFINITION;
    table[25] = &terrain::TERRAIN_DEFINITION;
    table[35] = &terrain::TERRAIN_DEFINITION;
    table[41] = &terrain::TERRAIN_DEFINITION;
    table[42] = &terrain::TERRAIN_DEFINITION;
    table[43] = &terrain::TERRAIN_DEFINITION;
    table[44] = &terrain::TERRAIN_DEFINITION;
    table[45] = &terrain::TERRAIN_DEFINITION;
    table[46] = &terrain::TERRAIN_DEFINITION;
    table[47] = &terrain::TERRAIN_DEFINITION;
    table[48] = &terrain::TERRAIN_DEFINITION;
    table[49] = &terrain::TERRAIN_DEFINITION;
    table[52] = &terrain::TERRAIN_DEFINITION;
    table[53] = &terrain::TERRAIN_DEFINITION;
    table[67] = &terrain::TERRAIN_DEFINITION;
    table[57] = &terrain::TERRAIN_DEFINITION;
    table[58] = &terrain::TERRAIN_DEFINITION;
    table[60] = &terrain::TERRAIN_DEFINITION;
    table[78] = &terrain::TERRAIN_DEFINITION;
    table[79] = &terrain::TERRAIN_DEFINITION;
    table[80] = &terrain::TERRAIN_DEFINITION;
    table[82] = &terrain::TERRAIN_DEFINITION;
    table[84] = &terrain::TERRAIN_DEFINITION;
    table[85] = &terrain::TERRAIN_DEFINITION;
    table[88] = &terrain::TERRAIN_DEFINITION;
    table[92] = &terrain::TERRAIN_DEFINITION;
    table[95] = &terrain::TERRAIN_DEFINITION;
    table[96] = &terrain::TERRAIN_DEFINITION;
    table[87] = &terrain::TERRAIN_DEFINITION;
    table[14] = &ores::ORE_DEFINITION;
    table[15] = &ores::ORE_DEFINITION;
    table[16] = &ores::ORE_DEFINITION;
    table[21] = &ores::ORE_DEFINITION;
    table[22] = &ores::ORE_DEFINITION;
    table[56] = &ores::ORE_DEFINITION;
    table[73] = &ores::ORE_DEFINITION;
    table[74] = &ores::ORE_DEFINITION;
    table[6] = &plants::PLANT_DEFINITION;
    table[17] = &plants::PLANT_DEFINITION;
    table[18] = &plants::PLANT_DEFINITION;
    table[31] = &plants::PLANT_DEFINITION;
    table[32] = &plants::PLANT_DEFINITION;
    table[37] = &plants::PLANT_DEFINITION;
    table[38] = &plants::PLANT_DEFINITION;
    table[39] = &plants::PLANT_DEFINITION;
    table[40] = &plants::PLANT_DEFINITION;
    table[59] = &plants::PLANT_DEFINITION;
    table[81] = &plants::PLANT_DEFINITION;
    table[83] = &plants::PLANT_DEFINITION;
    table[200] = &plants::PLANT_DEFINITION;
    table[201] = &plants::PLANT_DEFINITION;
    table[202] = &plants::PLANT_DEFINITION;
    table[203] = &plants::PLANT_DEFINITION;
    table[217] = &plants::PLANT_DEFINITION;
    table[218] = &plants::PLANT_DEFINITION;
    table[208] = &plants::PLANT_DEFINITION;
    table[8] = &fluids::FLUID_DEFINITION;
    table[9] = &fluids::FLUID_DEFINITION;
    table[10] = &fluids::FLUID_DEFINITION;
    table[11] = &fluids::FLUID_DEFINITION;
    table[50] = &lights::LIGHT_DEFINITION;
    table[51] = &lights::LIGHT_DEFINITION;
    table[75] = &lights::LIGHT_DEFINITION;
    table[76] = &lights::LIGHT_DEFINITION;
    table[89] = &lights::LIGHT_DEFINITION;
    table[91] = &lights::LIGHT_DEFINITION;
    table[204] = &lights::LIGHT_DEFINITION;
    table[205] = &lights::LIGHT_DEFINITION;
    table[206] = &lights::LIGHT_DEFINITION;
    table[207] = &lights::LIGHT_DEFINITION;
    table[54] = &direction::DIRECTION_DEFINITION;
    table[223] = &direction::DIRECTION_DEFINITION;
    table[224] = &direction::DIRECTION_DEFINITION;
    table[225] = &direction::DIRECTION_DEFINITION;
    table[226] = &direction::DIRECTION_DEFINITION;
    table[65] = &direction::DIRECTION_DEFINITION;
    table[227] = &direction::DIRECTION_DEFINITION;
    table[228] = &direction::DIRECTION_DEFINITION;
    table[229] = &direction::DIRECTION_DEFINITION;
    table[230] = &direction::DIRECTION_DEFINITION;
    table[61] = &direction::DIRECTION_DEFINITION;
    table[62] = &direction::DIRECTION_DEFINITION;
    table[209] = &direction::DIRECTION_DEFINITION;
    table[210] = &direction::DIRECTION_DEFINITION;
    table[211] = &direction::DIRECTION_DEFINITION;
    table[212] = &direction::DIRECTION_DEFINITION;
    table[213] = &direction::DIRECTION_DEFINITION;
    table[214] = &direction::DIRECTION_DEFINITION;
    table[215] = &direction::DIRECTION_DEFINITION;
    table[216] = &direction::DIRECTION_DEFINITION;
    table[86] = &direction::DIRECTION_DEFINITION;
    table[219] = &direction::DIRECTION_DEFINITION;
    table[220] = &direction::DIRECTION_DEFINITION;
    table[221] = &direction::DIRECTION_DEFINITION;
    table[222] = &direction::DIRECTION_DEFINITION;
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

pub(crate) fn definition(block: Block) -> &'static dyn BlockDefinition {
    if matches!(block, Block::Unknown(_)) {
        &utility::DEFINITION
    } else {
        DEFINITIONS[block.as_u8() as usize]
    }
}
