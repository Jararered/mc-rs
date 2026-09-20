//! Periodic console report of frame and world-streaming performance.

use bevy::diagnostic::Diagnostic;
use bevy::diagnostic::DiagnosticPath;
use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::EntityCountDiagnosticsPlugin;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;

use crate::world::chunk::WorldChunks;
use crate::world::streaming::StreamingPerf;
use crate::world::streaming::TimingStats;
use crate::world::streaming::WorldStreaming;
use crate::world::streaming::stream_chunks;

const PERF_INTERVAL_SECS: f32 = 10.0;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
        ))
        .insert_resource(PerfTimer(Timer::from_seconds(
            PERF_INTERVAL_SECS,
            TimerMode::Repeating,
        )))
        .add_systems(Update, print_perf_stats.after(stream_chunks));
    }
}

#[derive(Resource)]
struct PerfTimer(Timer);

fn print_perf_stats(
    time: Res<Time>,
    mut timer: ResMut<PerfTimer>,
    diagnostics: Res<DiagnosticsStore>,
    chunks: Option<Res<WorldChunks>>,
    streaming: Option<Res<WorldStreaming>>,
    perf: Option<ResMut<StreamingPerf>>,
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

    let loaded_chunks = chunks.as_ref().map_or(0, |chunks| chunks.len());
    let (rendered, generating, meshing) = streaming.as_ref().map_or((0, 0, 0), |streaming| {
        (
            streaming.rendered_mesh_count(),
            streaming.generating_job_count(),
            streaming.meshing_job_count(),
        )
    });

    let (generate, load, mesh) = match perf {
        Some(mut perf) => (perf.generate.take(), perf.load.take(), perf.mesh.take()),
        None => (
            TimingStats::default(),
            TimingStats::default(),
            TimingStats::default(),
        ),
    };

    info!(
        "performance ({PERF_INTERVAL_SECS:.0}s)\n  \
         fps             {fps}\n  \
         frame time      {frame_ms} ms\n  \
         frames          {frames}\n  \
         entities        {entities}\n  \
         chunks          {loaded_chunks} loaded, {generating} generating\n  \
         meshes          {rendered} rendered, {meshing} meshing\n  \
         chunk generate  {}\n  \
         chunk load      {}\n  \
         mesh            {}",
        fmt_timing(&generate),
        fmt_timing(&load),
        fmt_timing(&mesh),
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

fn fmt_timing(stats: &TimingStats) -> String {
    match (stats.average_ms(), stats.max_ms()) {
        (Some(average), Some(max)) => {
            format!("{average:.1} ms avg, {max:.1} ms max (n={})", stats.count())
        }
        _ => "n/a".into(),
    }
}
