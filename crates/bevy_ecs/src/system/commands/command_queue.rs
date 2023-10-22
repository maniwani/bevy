use std::mem::MaybeUninit;
// use std::num::NonZeroUsize;
use std::sync::Mutex;

use thread_local::ThreadLocal;

use bevy_ptr::{OwningPtr, Unaligned};

use crate::component::ComponentId;
use crate::entity::Entity;
use crate::storage::SparseArray;
use crate::system::Command;
use crate::world::World;

// TODO: modify entity command creation so it converts TypeId into ComponentId?
// TODO: split bundles into individual commands
// TODO: encounter Insert, check component ID exists in sparse set, if not, claim data ptr and change kind to skip
// TODO: clear IDs from diff that are in both added and removed, properly drop any values in the sparse set

/// Used to construct a [`TypeDef`].
#[derive(Clone)]
pub(crate) struct TypeDefBuilder {
    ids: Vec<ComponentId>,
}

impl TypeDefBuilder {
    fn new() -> Self {
        Self { ids: Vec::new() }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            ids: Vec::with_capacity(capacity),
        }
    }

    fn add(&mut self, id: ComponentId) {
        todo!()
    }

    fn remove(&mut self, id: ComponentId) {
        todo!()
    }

    fn build(mut self) -> TypeDef {
        self.ids.sort();
        self.ids.dedup();

        TypeDef {
            ids: self.ids.into_boxed_slice(),
        }
    }
}

/// A hashable representation of a set of components.
#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) struct TypeDef {
    ids: Box<[ComponentId]>,
}

impl TypeDef {
    /// The `TypeDef` representing no components.
    const EMPTY: TypeDef = TypeDef { ids: Box::new([]) };

    fn new_with(&self, id: ComponentId) -> Option<TypeDef> {
        if let Some(index) = self.find_insert(id, 0) {
            let n = self.ids.len();
            let mut new = TypeDefBuilder::with_capacity(n + 1);
            // memcpy, add, memcpy
            new.ids.extend_from_slice(&self.ids[0..index]);
            new.ids.push(id);
            new.ids.extend_from_slice(&self.ids[index..n]);
            Some(new.build())
        } else {
            None
        }
    }

    fn new_without(&self, id: ComponentId) -> Option<TypeDef> {
        let n = self.ids.len();
        let index = self.find(id, 0);

        if index.is_none() {
            return None;
        } else if n == 1 {
            return Some(Self::EMPTY);
        }

        let i = index.unwrap();

        let mut dst = TypeDefBuilder::with_capacity(n - 1);
        // memcpy, memcpy
        dst.ids.extend_from_slice(&self.ids[0..i]);
        dst.ids.extend_from_slice(&self.ids[(i + 1)..n]);
        Some(dst.build())
    }

    fn find_insert(&self, id_to_add: ComponentId, start: usize) -> Option<usize> {
        // // TODO: partition_point?
        // let index = start + self.ids[start..].partition_point(|&id| id < id_to_add);
        // if self.ids[index] == id_to_add {
        //     None
        // } else {
        //     Some(index)
        // }

        for (i, id) in self.ids.iter().skip(start).copied().enumerate() {
            // TODO: ecs_id_match
            if id == id_to_add {
                return None;
            } else if id > id_to_add {
                return Some(i);
            }
        }

        Some(self.ids.len())
    }

    fn find(&self, id_to_remove: ComponentId, start: usize) -> Option<usize> {
        // // TODO: partition_point?
        // let index = start + self.ids[start..].partition_point(|&id| id < id_to_remove);
        // if self.ids[index] == id_to_add {
        //     Some(index)
        // } else {
        //     None
        // }

        for (i, id) in self.ids.iter().skip(start).copied().enumerate() {
            // TODO: ecs_id_match
            if id == id_to_remove {
                return Some(i);
            } else if id > id_to_remove {
                return None;
            }
        }

        None
    }
}

enum Init {
    /// The component will be initialized with the provided value.
    Move(CommandDataPtr),
    /// The component will be initialized with its `Default` or `Clone` constructor.
    Ctor,
}

#[derive(Clone)]
pub(crate) struct TypeDiffBuilder {
    added: Vec<ComponentId>,
    removed: Vec<ComponentId>,
}

impl TypeDiffBuilder {
    fn new() -> Self {
        Self {
            added: Vec::new(),
            removed: Vec::new(),
        }
    }

    fn with_capacity(added: usize, removed: usize) -> Self {
        Self {
            added: Vec::with_capacity(added),
            removed: Vec::with_capacity(removed),
        }
    }

    fn build(mut self) -> TypeDiff {
        self.added.sort();
        self.added.dedup();
        self.removed.sort();
        self.removed.dedup();

        TypeDiff {
            added: self.added.into_boxed_slice(),
            removed: self.removed.into_boxed_slice(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TypeDiff {
    added: Box<[ComponentId]>,
    removed: Box<[ComponentId]>,
}

enum EdgeContent {
    Component(ComponentId),
    Diff(TypeDiff),
}

fn compute_table_diff(
    src_archetype: &Archetype,
    dst_archetype: &Archetype,
    id: ComponentId,
) -> EdgeContent {
    let src_ids = src_archetype.components();
    let dst_ids = dst_archetype.components();

    let mut src_index = 0;
    let src_count = src_ids.len();

    let mut dst_index = 0;
    let dst_count = dst_ids.len();

    let mut added_count = 0;
    let mut removed_count = 0;

    let edge_is_complex = false; // id.lhs() == EcsIsA;

    // First compute the size of the diff, so we only have to allocate once.
    //
    // There is an efficient algorithm to compute the intersection of two sorted lists with linear
    // time complexity. That algorithm has been altered to instead compute their difference.
    while src_index < src_count && dst_index < dst_count {
        let src_id = src_ids[src_index];
        let dst_id = dst_ids[dst_index];

        // `dst_id` is missing from source.
        let added = dst_id < src_id;
        // `src_id` is missing from destination.
        let removed = src_id < dst_id;

        // The edge is "complex" if it adds or removes components other than `id`.
        edge_is_complex |= added && (dst_id != id);
        edge_is_complex |= removed && (src_id != id);

        added_count += added as usize;
        removed_count += removed as usize;

        src_index += (src_id <= dst_id) as usize;
        dst_index += (dst_id <= src_id) as usize;
    }

    // When we reach the end of one list, we know all remaining components will be included (added
    // if src is exhausted first, removed if dst is exhausted first).
    let dst_remaining = dst_count - dst_index;
    let src_remaining = src_count - src_index;

    added_count += dst_remaining;
    removed_count += src_remaining;

    // The edge is "complex" if multiple components are changing.
    edge_is_complex |= (added_count + removed_count) > 1;
    // The edge is "complex" if it is removing a wildcard or wildcarded pair.
    // edge_is_complex |= id.is_wildcard();

    if !edge_is_complex {
        return EdgeContent::Component(id);
    }

    let mut builder = TypeDiffBuilder::with_capacity(added_count, removed_count);
    src_index = 0;
    dst_index = 0;

    while src_index < src_count && dst_index < dst_count {
        let src_id = src_ids[src_index];
        let dst_id = dst_ids[dst_index];

        if dst_id < src_id {
            builder.added.push(dst_id);
        } else if src_id < dst_id {
            builder.removed.push(src_id);
        }

        src_index += (src_id <= dst_id) as usize;
        dst_index += (dst_id <= src_id) as usize;
    }

    // memcpy
    builder
        .added
        .extend_from_slice(&dst_ids[dst_index..dst_count]);
    builder
        .removed
        .extend_from_slice(&src_ids[src_index..src_count]);

    let diff = builder.build();
    EdgeContent::Diff(diff)
}

pub(crate) enum NewCommandMeta {
    Entity(EntityCommandMeta),
    Custom(CustomCommandMeta),
}

pub(crate) enum EntityCommandKind {
    Spawn,
    Insert(ComponentId, CommandDataPtr),
    Remove(ComponentId),
    Despawn,
    Skip,
}

struct EntityCommandMeta {
    entity: Entity,
    next_command_for_entity: Option<usize>,
    kind: EntityCommandKind,
}

struct CustomCommandMeta {
    /// SAFETY: The `value` must point to a value of type `T: Command`,
    /// where `T` is some specific type that was used to produce this metadata.
    ///
    /// Returns the size of `T` in bytes.
    apply_command_and_get_size: unsafe fn(value: OwningPtr<Unaligned>, world: &mut World) -> usize,
}

pub struct NewCommandQueue {
    queue: Vec<NewCommandMeta>,
    alloc: CommandDataAllocator,
}

#[derive(Default)]
pub struct CommandDataAllocator {
    /// This buffer densely stores all command data.
    ///
    /// Commands can only be applied with exclusive access to the [`World`] and systems might issue
    /// hundreds of them, so they need to be extremely efficient at scale. `Vec<Box<dyn Command>>`
    /// allocates new memory for every single command, which has signifant overhead. To avoid that,
    /// we manually manage and recycle the memory owned by this `Vec<MaybeUninit<u8>>`.
    bytes: Vec<MaybeUninit<u8>>,
}

struct Staging {
    last_command_for_entity: SparseArray<Entity, CommandDataPtr>,
}

struct CommandDataPtr(usize);

pub fn batch_commands_for_entity(entity: Entity, first_command: usize) {
    use EntityCommandKind::*;
    let mut next_command_for_entity = Some(first_command);
    while let Some(command_index) = next_command_for_entity {
        let command: EntityCommandMeta;

        // assert!(command.is_entity_command());
        match command.kind {
            Spawn => {
                // dst = ArchetypeId::EMPTY;
            }
            Insert(id, value) => {
                // dst = traverse edge
            }
            Remove(id) => {
                // dst = traverse edge
            }
            Despawn => {
                // dst = ArchetypeId::INVALID;
            }
            Skip => (),
        }

        next_command_for_entity = command.next_command_for_entity;
    }

    // trace!("batched {} commands for entity {}", n, entity);

    // move entity to its new archetype
}

fn batch_commands(
    &mut self,
    entity: Entity,
    commands: &mut Commands,
    index: usize,
    diff: &mut TypeDiffBuilder,
) {
    let start_table = if let Some(location) = self.entities.get_location(entity) {
        location.table_id
    } else {
        TableId::EMPTY
    };

    let mut table = start_table;

    // Track the diff to run the appropriate hooks.
    let mut diff = TypeDiffBuilder::new();

    let mut next_command_for_entity = Some(zig_zag_encode(index));
    while let Some(encoded_index) = next_command_for_entity {
        let index = zig_zag_decode(encoded_index);
        let mut command = unsafe { commands.get_mut_unchecked(index) };

        match command.kind {
            Add { ids, .. } => {
                // TODO: Modified
                // If entity already has a component, and the component has an OnSet hook (type-level), we
                // need to invoke it here. This will ensure that any OnAdd/OnRemove observers see the most
                // recent value.
                table.find_add_component_destination(command.id, diff);
                command.kind = Skip;
            }
            Set { ids, .. } => {
                has_set = true;
                table.find_add_component_destination(command.id, diff);
            }
            Remove { ids } => {
                table.find_remove_component_destination(command.id, diff);
                command.kind = Skip;
            }
            RemoveAll => {
                // Commands can only remove components that the entity had.
                diff.removed.clear();
                diff.removed.reserve(start_table.component_count());
                diff.removed.extend_from_slice(start_table.components());
                command.kind = Skip;
            }
            _ => (),
        }
    }

    // Move entity to its final destination.
    diff.build();
    world.deferred(|world| {
        world.commit(/* ... */);
    });
}

struct CommandMeta {
    /// SAFETY: The `value` must point to a value of type `T: Command`,
    /// where `T` is some specific type that was used to produce this metadata.
    ///
    /// Returns the size of `T` in bytes.
    apply_command_and_get_size: unsafe fn(value: OwningPtr<Unaligned>, world: &mut World) -> usize,
}

/// An append-only queue that densely and efficiently stores heterogenous types implementing
/// [`Command`].
#[derive(Default)]
pub struct RawCommandQueue {
    /// This buffer densely stores all queued commands.
    ///
    /// Commands can only be applied with exclusive access to the [`World`] and systems might issue
    /// hundreds of them, so they need to be extremely efficient at scale. `Vec<Box<dyn Command>>`
    /// allocates new memory for every single command, which has signifant overhead. To avoid that,
    /// we manually manage and recycle the memory owned by the `Vec<MaybeUninit<u8>>`.
    ///
    /// For each command, a `CommandMeta` is stored, followed by zero or more bytes that store the
    /// command itself. To interpret these bytes, a pointer must be passed to the corresponding
    /// `CommandMeta::apply_command_and_get_size` function pointer.
    bytes: Vec<MaybeUninit<u8>>,
    /// The number of commands in the queue.
    count: usize,
}

// SAFETY: All types that implement [`Command`] are required to implement [`Send`].
unsafe impl Send for RawCommandQueue {}

// SAFETY: `&RawCommandQueue` never gives access to the inner commands.
unsafe impl Sync for RawCommandQueue {}

impl RawCommandQueue {
    /// Push a [`Command`] onto the queue.
    #[inline]
    pub fn push<C>(&mut self, command: C)
    where
        C: Command,
    {
        // Stores a command alongside its metadata.
        // `repr(C)` prevents the compiler from reordering the fields,
        // while `repr(packed)` prevents the compiler from inserting padding bytes.
        #[repr(C, packed)]
        struct Packed<T: Command> {
            meta: CommandMeta,
            command: T,
        }

        let meta = CommandMeta {
            apply_command_and_get_size: |command, world| {
                // SAFETY: According to the invariants of `CommandMeta.apply_command_and_get_size`,
                // `command` must point to a value of type `C`.
                let command: C = unsafe { command.read_unaligned() };
                command.apply(world);
                std::mem::size_of::<C>()
            },
        };

        let old_len = self.bytes.len();

        // Reserve enough bytes for both the metadata and the command itself.
        self.bytes.reserve(std::mem::size_of::<Packed<C>>());

        // Pointer to the bytes at the end of the buffer.
        // SAFETY: We know it is within bounds of the allocation, due to the call to `.reserve()`.
        let ptr = unsafe { self.bytes.as_mut_ptr().add(old_len) };

        // Write the metadata into the buffer, followed by the command.
        // We are using a packed struct to write them both as one operation.
        // SAFETY: `ptr` must be non-null, since it is within a non-null buffer.
        // The call to `reserve()` ensures that the buffer has enough space to fit a value of type `C`,
        // and it is valid to write any bit pattern since the underlying buffer is of type `MaybeUninit<u8>`.
        unsafe {
            ptr.cast::<Packed<C>>()
                .write_unaligned(Packed { meta, command });
        }

        // Extend the length of the buffer to include the data we just wrote.
        // SAFETY: The new length is guaranteed to be valid from the call to `.reserve()` above.
        unsafe {
            self.bytes
                .set_len(old_len + std::mem::size_of::<Packed<C>>());
        }

        self.count += 1;
    }

    /// Applies all queued commands on the world, in insertion order, then clears the queue.
    #[inline]
    pub fn apply(&mut self, world: &mut World) {
        // flush the previously queued entities
        world.flush();

        // Pointer that will iterate over the entries of the buffer.
        let mut cursor = self.bytes.as_mut_ptr();

        // The address of the end of the buffer.
        let end_addr = cursor as usize + self.bytes.len();

        while (cursor as usize) < end_addr {
            // SAFETY: The cursor is either at the start of the buffer, or just after the previous command.
            // Since we know that the cursor is in bounds, it must point to the start of a new command.
            let meta = unsafe { cursor.cast::<CommandMeta>().read_unaligned() };
            // Advance to the bytes just after `meta`, which represent a type-erased command.
            // SAFETY: For most types of `Command`, the pointer immediately following the metadata
            // is guaranteed to be in bounds. If the command is a zero-sized type (ZST), then the cursor
            // might be 1 byte past the end of the buffer, which is safe.
            cursor = unsafe { cursor.add(std::mem::size_of::<CommandMeta>()) };
            // Construct an owned pointer to the command.
            // SAFETY: It is safe to transfer ownership out of `self.bytes`, since the call to `set_len(0)` above
            // guarantees that nothing stored in the buffer will get observed after this function ends.
            // `cmd` points to a valid address of a stored command, so it must be non-null.
            let cmd = unsafe {
                OwningPtr::<Unaligned>::new(std::ptr::NonNull::new_unchecked(cursor.cast()))
            };
            // SAFETY: The data underneath the cursor must correspond to the type erased in metadata,
            // since they were stored next to each other by `.push()`.
            // For ZSTs, the type doesn't matter as long as the pointer is non-null.
            let size = unsafe { (meta.apply_command_and_get_size)(cmd, world) };
            // Advance the cursor past the command. For ZSTs, the cursor will not move.
            // At this point, it will either point to the next `CommandMeta`,
            // or the cursor will be out of bounds and the loop will end.
            // SAFETY: The address just past the command is either within the buffer,
            // or 1 byte past the end, so this addition will not overflow the pointer's allocation.
            cursor = unsafe { cursor.add(size) };
        }

        // Reset the buffer, so it can be reused after this function ends.
        // In the loop above, ownership of each command was transferred into user code.
        // SAFETY: `set_len(0)` is always valid.
        unsafe {
            std::ptr::write_bytes(&mut self.bytes, 0, self.bytes.len());
            self.bytes.set_len(0);
        };

        self.count = 0;
    }

    /// Moves all the commands of `other` into `self`, leaving `other` empty.
    pub(crate) fn append(&mut self, other: &mut Self) {
        // sum lengths and counts
        let new_len = self.bytes.len() + other.bytes.len();
        let new_count = self.count + other.count;

        // Vec::append but using memcpy
        self.bytes.reserve(other.bytes.len());
        let spare_bytes = self.bytes.spare_capacity_mut();
        // TODO: can replace with MaybeUninit<T>::write_slice if it stabilizes
        // SAFETY: [MaybeUninit<MaybeUninit<T>>] is equivalent to [MaybeUninit<T>].
        let spare_bytes: &mut [MaybeUninit<u8>] = unsafe { std::mem::transmute(spare_bytes) };
        spare_bytes.copy_from_slice(&other.bytes);

        // Ownership of each command was transferred to `self`.
        // SAFETY: `set_len(0)` is always valid.
        unsafe {
            std::ptr::write_bytes(&mut other.bytes, 0, other.bytes.len());
            other.bytes.set_len(0);
        };

        other.count = 0;

        // SAFETY: The new length is guaranteed to be valid from the call to `.reserve()` above.
        unsafe {
            self.bytes.set_len(new_len);
        }

        self.count = new_count;
    }
}

/// A thread-safe command queue (stores one per thread).
#[derive(Default)]
pub struct CommandQueue {
    queues: ThreadLocal<Mutex<RawCommandQueue>>,
}

impl CommandQueue {
    /// TODO
    pub fn scope<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut RawCommandQueue) -> R,
    {
        let mutex = self.queues.get_or_default();
        let mut queue = mutex.try_lock().expect("no nested `scope` calls");
        f(&mut *queue)
    }

    /// Applies all queued commands on the world, in insertion order, then clears the queue.
    pub fn apply(&mut self, world: &mut World) {
        for mutex in self.queues.iter_mut() {
            let mut queue = mutex.try_lock().unwrap();
            queue.apply(world);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{
        panic::AssertUnwindSafe,
        sync::{
            atomic::{AtomicU32, Ordering},
            Arc,
        },
    };

    struct DropCheck(Arc<AtomicU32>);

    impl DropCheck {
        fn new() -> (Self, Arc<AtomicU32>) {
            let drops = Arc::new(AtomicU32::new(0));
            (Self(drops.clone()), drops)
        }
    }

    impl Drop for DropCheck {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl Command for DropCheck {
        fn apply(self, _: &mut World) {}
    }

    #[test]
    fn test_command_queue_inner_drop() {
        let mut queue = RawCommandQueue::default();

        let (dropcheck_a, drops_a) = DropCheck::new();
        let (dropcheck_b, drops_b) = DropCheck::new();

        queue.push(dropcheck_a);
        queue.push(dropcheck_b);

        assert_eq!(drops_a.load(Ordering::Relaxed), 0);
        assert_eq!(drops_b.load(Ordering::Relaxed), 0);

        let mut world = World::new();
        queue.apply(&mut world);

        assert_eq!(drops_a.load(Ordering::Relaxed), 1);
        assert_eq!(drops_b.load(Ordering::Relaxed), 1);
    }

    struct SpawnCommand;

    impl Command for SpawnCommand {
        fn apply(self, world: &mut World) {
            world.spawn_empty();
        }
    }

    #[test]
    fn test_command_queue_inner() {
        let mut queue = RawCommandQueue::default();

        queue.push(SpawnCommand);
        queue.push(SpawnCommand);

        let mut world = World::new();
        queue.apply(&mut world);

        assert_eq!(world.entities().len(), 2);

        // The previous call to `apply` cleared the queue.
        // This call should do nothing.
        queue.apply(&mut world);
        assert_eq!(world.entities().len(), 2);
    }

    // This has an arbitrary value `String` stored to ensure
    // when then command gets pushed, the `bytes` vector gets
    // some data added to it.
    struct PanicCommand(String);
    impl Command for PanicCommand {
        fn apply(self, _: &mut World) {
            panic!("command is panicking");
        }
    }

    #[test]
    fn test_command_queue_inner_panic_safe() {
        std::panic::set_hook(Box::new(|_| {}));

        let mut queue = RawCommandQueue::default();

        queue.push(PanicCommand("I panic!".to_owned()));
        queue.push(SpawnCommand);

        let mut world = World::new();

        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            queue.apply(&mut world);
        }));

        // even though the first command panicking.
        // the `bytes`/`metas` vectors were cleared.
        assert_eq!(queue.bytes.len(), 0);

        // Even though the first command panicked, it's still ok to push
        // more commands.
        queue.push(SpawnCommand);
        queue.push(SpawnCommand);
        queue.apply(&mut world);
        assert_eq!(world.entities().len(), 2);
    }

    // NOTE: `RawCommandQueue` is `Send` because `Command` is send.
    // If the `Command` trait gets reworked to be non-send, `RawCommandQueue`
    // should be reworked.
    // This test asserts that Command types are send.
    fn assert_is_send_impl(_: impl Send) {}
    fn assert_is_send(command: impl Command) {
        assert_is_send_impl(command);
    }

    #[test]
    fn test_command_is_send() {
        assert_is_send(SpawnCommand);
    }

    struct CommandWithPadding(u8, u16);
    impl Command for CommandWithPadding {
        fn apply(self, _: &mut World) {}
    }

    #[cfg(miri)]
    #[test]
    fn test_uninit_bytes() {
        let mut queue = RawCommandQueue::default();
        queue.push(CommandWithPadding(0, 0));
        let _ = format!("{:?}", queue.bytes);
    }
}
