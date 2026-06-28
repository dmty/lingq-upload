use lingq_upload_lib::core::project::ProjectStage;
use lingq_upload_lib::events::Stage;

fn wants_event_stage(_s: Stage) {}
fn wants_project_stage(_s: ProjectStage) {}

fn main() {
    wants_event_stage(ProjectStage::New);
    wants_project_stage(Stage::Transcoding);
}
