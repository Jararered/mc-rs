//! Item identities, definitions, and validated inventory stacks.
pub mod registry;
pub mod tools;
use crate::block::blocks::Block;
pub use registry::ItemData;
pub use registry::ItemId;
pub use registry::ItemProperties;
pub use registry::ItemRegistry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackError {
    UnknownItem(ItemId),
    InvalidCount { count: u8, max: u8 },
    InvalidData(u16),
}

impl std::fmt::Display for StackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownItem(id) => write!(f, "unknown item ID {}", id.as_u16()),
            Self::InvalidCount { count, max } => {
                write!(f, "stack count {count} is outside 1..={max}")
            }
            Self::InvalidData(data) => write!(f, "unsupported item data {data}"),
        }
    }
}
impl std::error::Error for StackError {}

/// Nonempty stack. Private fields preserve registry limits; an empty slot is None.
/// Data stores subtype, tool damage, or map identity according to the definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    item: ItemId,
    count: u8,
    data: u16,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u8) -> Result<Self, StackError> {
        Self::with_data(item, count, 0)
    }
    pub fn with_data(item: ItemId, count: u8, data: u16) -> Result<Self, StackError> {
        let definition = item.properties().ok_or(StackError::UnknownItem(item))?;
        if count == 0 || count > definition.max_stack_size {
            return Err(StackError::InvalidCount {
                count,
                max: definition.max_stack_size,
            });
        }
        if !definition.data.accepts(data) {
            return Err(StackError::InvalidData(data));
        }
        Ok(Self { item, count, data })
    }

    /// Direct block representation, not a mining-drop rule (stone may drop cobble).
    /// Torch attachment is discarded; species metadata is retained.
    pub fn from_block(block: Block, count: u8) -> Result<Self, StackError> {
        let (item_block, data) = block.item_form();
        let Some(item) = ItemId::from_block(item_block) else {
            return Err(StackError::UnknownItem(ItemId::BlockOrUnknown(u16::from(
                item_block.as_u8(),
            ))));
        };
        Self::with_data(item, count, u16::from(data))
    }
    pub const fn item(self) -> ItemId {
        self.item
    }
    pub const fn count(self) -> u8 {
        self.count
    }
    pub const fn data(self) -> u16 {
        self.data
    }
    pub fn with_count(self, count: u8) -> Result<Self, StackError> {
        Self::with_data(self.item, count, self.data)
    }
    pub fn container_item(self) -> Option<ItemId> {
        self.definition().container_item()
    }
    pub fn definition(self) -> ItemProperties {
        self.item.properties().expect("validated stack identity")
    }

    /// Direct placement candidate for implemented block states. Special items
    /// such as doors require their own use behavior.
    pub fn runtime_block(self) -> Option<Block> {
        let block = self.definition().block?;
        block.placed(u8::try_from(self.data).ok()?)
    }

    /// Beta `ItemStack.damageItem`. `None` means the stack broke.
    ///
    /// Uses may equal max damage. The item is destroyed only when damage
    /// exceeds it, matching `itemDamage > getMaxDamage()`.
    pub fn apply_damage(self, amount: u16) -> Option<Self> {
        let ItemData::Durability(max) = self.definition().data else {
            return Some(self);
        };
        if amount == 0 {
            return Some(self);
        }
        let next = u32::from(self.data) + u32::from(amount);
        if next > u32::from(max) {
            let left = self.count.saturating_sub(1);
            if left == 0 {
                None
            } else {
                Self::with_data(self.item, left, 0).ok()
            }
        } else {
            Self::with_data(self.item, self.count, next as u16).ok()
        }
    }

    pub fn can_merge(self, other: Self) -> bool {
        self.item == other.item
            && self.data == other.data
            && self.count < self.definition().max_stack_size
    }

    /// Fill this stack and return any unconsumed input, never a zero-count stack.
    pub fn merge(&mut self, mut other: Self) -> Option<Self> {
        if !self.can_merge(other) {
            return Some(other);
        }
        let moved = other
            .count
            .min(self.definition().max_stack_size - self.count);
        self.count += moved;
        other.count -= moved;
        (other.count != 0).then_some(other)
    }
}
