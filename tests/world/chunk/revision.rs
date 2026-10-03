use game::block::id::Id;
use game::world::chunk::Chunk;

#[test]
fn revision_advances_with_every_block_and_metadata_write() {
    let mut chunk = Chunk::new();
    let mut seen = chunk.revision();
    let mut advanced = |chunk: &Chunk| {
        let moved = chunk.revision() != seen;
        seen = chunk.revision();
        moved
    };
    chunk.set(1, 2, 3, Id::Stone);
    assert!(advanced(&chunk));
    chunk.set_with_metadata(1, 3, 3, Id::Water, 2);
    assert!(advanced(&chunk));
    chunk.set_metadata(1, 3, 3, 5);
    assert!(advanced(&chunk));
    // Writing the metadata a cell already has changes nothing.
    chunk.set_metadata(1, 3, 3, 5);
    assert!(!advanced(&chunk));
    chunk.set_raw_metadata(Vec::new());
    assert!(advanced(&chunk));
}
