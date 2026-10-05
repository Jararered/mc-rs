//! Periodic console report of frame and world-streaming performance.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticPath;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::EntityCountDiagnosticsPlugin;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::diagnostic::SystemInformationDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::diagnostic::MeshAllocatorDiagnosticPlugin;

use crate::entity::EntityDiagnostics;
use crate::rendering::chunk_quads::ChunkQuads;
use crate::world::chunk::WorldChunks;
use crate::world::streaming::StreamingDiagnostics;
use crate::world::streaming::TimingStats;
use crate::world::streaming::WorldStreaming;
use crate::world::streaming::stream_chunks;

const DIAGNOSTICS_INTERVAL_SECS: f32 = 10.0;

pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
            // Resident memory of the process for the report.
            SystemInformationDiagnosticsPlugin,
            // Chunk meshes live in Bevy's shared mesh slabs.
            MeshAllocatorDiagnosticPlugin,
        ))
        .insert_resource(DiagnosticsTimer(Timer::from_seconds(
            DIAGNOSTICS_INTERVAL_SECS,
            TimerMode::Repeating,
        )))
        .init_resource::<FrameSpikes>()
        .add_systems(First, track_frame_spikes)
        .add_systems(Update, print_perf_stats.after(stream_chunks));
    }
}

#[derive(Resource)]
struct DiagnosticsTimer(Timer);

/// Real frame times since the last report. The frame-time diagnostics are
/// smoothed, which hides a single stutter.
#[derive(Resource, Default)]
struct FrameSpikes(Vec<f32>);

impl FrameSpikes {
    /// The slowest frame, and how many frames took over twice the mean. The
    /// mean follows the frame cap and the slower unfocused pacing, so neither
    /// counts as a stutter.
    fn take(&mut self) -> (f32, usize) {
        let frames = std::mem::take(&mut self.0);
        let mean = frames.iter().sum::<f32>() / frames.len().max(1) as f32;
        let slowest = frames.iter().copied().fold(0.0, f32::max);
        let long = frames.iter().filter(|ms| **ms > mean * 2.0).count();
        (slowest, long)
    }
}

fn track_frame_spikes(time: Res<Time<Real>>, mut spikes: ResMut<FrameSpikes>) {
    spikes.0.push(time.delta_secs() * 1000.0);
}

fn print_perf_stats(
    time: Res<Time>,
    mut timer: ResMut<DiagnosticsTimer>,
    diagnostics: Res<DiagnosticsStore>,
    chunks: Option<Res<WorldChunks>>,
    streaming: Option<Res<WorldStreaming>>,
    perf: Option<ResMut<StreamingDiagnostics>>,
    entity_perf: Option<ResMut<EntityDiagnostics>>,
    quads: Option<Res<ChunkQuads>>,
    mut spikes: ResMut<FrameSpikes>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let fps = fmt_diag(&diagnostics, &FrameTimeDiagnosticsPlugin::FPS, 1);
    let frame_ms = fmt_diag(&diagnostics, &FrameTimeDiagnosticsPlugin::FRAME_TIME, 2);
    let frames = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_COUNT)
        .and_then(Diagnostic::value)
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| "n/a".into());
    let entities = fmt_diag(&diagnostics, &EntityCountDiagnosticsPlugin::ENTITY_COUNT, 0);
    let (slowest_ms, long) = spikes.take();
    // Bevy reports GiB. This is resident memory, which on macOS leaves out
    // most GPU allocations; `footprint` shows those.
    let process_mib = fmt_latest(
        &diagnostics,
        &SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE,
        1.0 / 1024.0,
        0,
    );

    let loaded_chunks = chunks.as_ref().map_or(0, |chunks| chunks.len());
    let (rendered, layers, mesh_bytes, generating, populating, meshing) =
        streaming.as_ref().map_or((0, 0, 0, 0, 0, 0), |streaming| {
            (
                streaming.rendered_mesh_count(),
                streaming.rendered_layer_count(),
                streaming.mesh_bytes(),
                streaming.generating_job_count(),
                streaming.populating_job_count(),
                streaming.meshing_job_count(),
            )
        });
    let mesh_mib = mesh_bytes as f64 / (1024.0 * 1024.0);
    let layer_format = if streaming.as_ref().is_some_and(|s| s.quad_layers()) {
        "quad records"
    } else {
        "vertex and index data"
    };
    let quad_buffer = quads.map_or_else(
        || "n/a".into(),
        |quads| {
            let (reserved, used) = quads.memory();
            format!(
                "{:.1} MiB reserved, {:.1} MiB in use",
                reserved as f64 / (1024.0 * 1024.0),
                used as f64 / (1024.0 * 1024.0),
            )
        },
    );
    let slabs = fmt_latest(
        &diagnostics,
        MeshAllocatorDiagnosticPlugin::slabs_diagnostic_path(),
        1.0,
        0,
    );
    let slab_mib = fmt_latest(
        &diagnostics,
        MeshAllocatorDiagnosticPlugin::slabs_size_diagnostic_path(),
        1024.0 * 1024.0,
        1,
    );
    let allocations = fmt_latest(
        &diagnostics,
        MeshAllocatorDiagnosticPlugin::allocations_diagnostic_path(),
        1.0,
        0,
    );

    let (generate, populate, load, mesh, discovery_passes) = match perf {
        Some(mut perf) => (
            perf.generate.take(),
            perf.populate.take(),
            perf.load.take(),
            perf.mesh.take(),
            std::mem::take(&mut perf.discovery_passes),
        ),
        None => (
            TimingStats::default(),
            TimingStats::default(),
            TimingStats::default(),
            TimingStats::default(),
            0,
        ),
    };

    let entity = entity_perf.map(|mut perf| perf.take()).unwrap_or_default();
    let per_search = if entity.searches.searches > 0 {
        entity.searches.nodes / entity.searches.searches
    } else {
        0
    };

    info!(
        "performance ({DIAGNOSTICS_INTERVAL_SECS:.0}s)\n  \
         fps             {fps}\n  \
         frame time      {frame_ms} ms\n  \
         slowest frame   {slowest_ms:.2} ms, {long} frames over twice the mean\n  \
         frames          {frames}\n  \
         entities        {entities}\n  \
         process memory  {process_mib} MiB resident\n  \
         chunks          {loaded_chunks} loaded, {generating} generating, {populating} populating\n  \
         meshes          {rendered} chunks, {layers} section layers, {meshing} meshing\n  \
         mesh memory     {mesh_mib:.1} MiB {layer_format}\n  \
         quad buffer     {quad_buffer}\n  \
         mesh slabs      {slabs} slabs, {slab_mib} MiB reserved, {allocations} allocations\n  \
         streaming scans {discovery_passes} candidate-discovery passes\n  \
         chunk generate  {}\n  \
         chunk populate  {}\n  \
         chunk load      {}\n  \
         mesh            {}\n  \
         mobs            {} ({} model boxes)\n  \
         mob ticks       {} over {} ticks\n  \
         pathfinding     {} searches, {} nodes ({per_search} per search), {} repeats reused\n  \
         mob spawning    {}\n  \
         mob posing      {}",
        fmt_timing(&generate),
        fmt_timing(&populate),
        fmt_timing(&load),
        fmt_timing(&mesh),
        entity.mobs,
        entity.parts,
        fmt_timing(&entity.creatures),
        entity.ticks,
        entity.searches.searches,
        entity.searches.nodes,
        entity.searches.reused,
        fmt_timing(&entity.spawning),
        fmt_timing(&entity.posing),
    );
}

fn fmt_diag(store: &DiagnosticsStore, path: &DiagnosticPath, digits: usize) -> String {
    let Some(diagnostic) = store.get(path) else {
        return "n/a".into();
    };
    let Some(smoothed) = diagnostic.smoothed().or_else(|| diagnostic.value()) else {
        return "n/a".into();
    };
    match diagnostic.average() {
        Some(average) if (average - smoothed).abs() > 0.05 => {
            format!("{smoothed:.digits$} (avg {average:.digits$})")
        }
        _ => format!("{smoothed:.digits$}"),
    }
}

/// The newest sample of a count or size, which should not be smoothed.
fn fmt_latest(store: &DiagnosticsStore, path: &DiagnosticPath, unit: f64, digits: usize) -> String {
    store.get(path).and_then(Diagnostic::value).map_or_else(
        || "n/a".into(),
        |value| format!("{:.digits$}", value / unit),
    )
}

fn fmt_timing(stats: &TimingStats) -> String {
    match (stats.average_ms(), stats.max_ms()) {
        (Some(average), Some(max)) => {
            format!("{average:.2} ms avg, {max:.2} ms max (n={})", stats.count())
        }
        _ => "n/a".into(),
    }
}
