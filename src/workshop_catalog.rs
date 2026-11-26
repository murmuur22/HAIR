//! Draft-only catalog form. Project remains the only persistence/export authority.
use super::{DialogKind, Workshop};
use crate::{Result, decoration::{Asset, Defaults, Entry}};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Draft {
    target: Option<String>,
    id: String,
    tag: String,
    name: String,
    asset: String,
    file: String,
    collision: bool,
    extent: String,
    scale: String,
    yaw: String,
    pivot: [String; 3],
}
impl Default for Draft {
    fn default() -> Self {
        Self { target:None, id:String::new(), tag:String::new(), name:String::new(), asset:String::new(), file:String::new(), collision:false,
            extent:"10".into(), scale:"1".into(), yaw:"0".into(), pivot:std::array::from_fn(|_|"0".into()) }
    }
}
impl Draft {
    fn values(&self) -> Result<(Asset, Entry)> {
        fn number(value:&str, label:&str) -> Result<f32> {
            value.parse::<f32>().ok().filter(|v|v.is_finite()).ok_or_else(||format!("{label}: enter a finite number"))
        }
        Ok((Asset { id:self.asset.clone(), file:self.file.clone(), max_extent:number(&self.extent,"Maximum extent")?, scale:number(&self.scale,"Scale")?,
            yaw_degrees:number(&self.yaw,"Yaw")?, pivot:[number(&self.pivot[0],"Pivot X")?,number(&self.pivot[1],"Pivot Y")?,number(&self.pivot[2],"Pivot Z")?] },
            Entry { id:self.id.clone(), tag:self.tag.clone(), name:self.name.clone(), asset:self.asset.clone(), defaults:Defaults {collision:self.collision} }))
    }
    fn use_asset(&mut self, a:&Asset) {
        self.asset=a.id.clone(); self.file=a.file.clone(); self.extent=a.max_extent.to_string(); self.scale=a.scale.to_string(); self.yaw=a.yaw_degrees.to_string();
        self.pivot=a.pivot.map(|v|v.to_string());
    }
}
#[derive(Clone)]
pub(super) struct Snapshot {
    revision:u64,
    selection:Option<String>,
    draft:Draft,
    category:&'static str,
    kind:DialogKind,
    asset:Option<Asset>,
}
impl Workshop {
    pub fn selected_catalog_entry(&self) -> Option<&str> { self.catalog_selection.as_deref() }
    pub(super) fn catalog_snapshot(&self,kind:DialogKind) -> Snapshot {
        let asset=self.catalog_selection.as_deref().and_then(|id| {
            let s=self.project.as_ref()?.catalog()?;
            let entry=s.entries.iter().find(|e|e.id==id)?;
            s.assets.iter().find(|a|a.id==entry.asset).cloned()
        });
        Snapshot { revision:self.revision, selection:self.catalog_selection.clone(), draft:self.catalog_draft.clone(), category:self.category, kind, asset }
    }
    pub(super) fn finish_catalog_pick(&mut self, snapshot:Snapshot, path:&Path) -> Result<()> {
        if self.revision!=snapshot.revision || self.catalog_selection!=snapshot.selection || self.category!=snapshot.category || self.catalog_draft!=snapshot.draft {
            return Err("Catalog context changed while chooser was open; selected file was not applied".into());
        }
        // Bounded event-time candidate, never a second export authority. Keep
        // last-good source bytes as well as metadata if shared placement refuses.
        let mut candidate=self.project.as_ref().ok_or("Start a pack first")?.clone();
        let p=&mut candidate;
        let theme=match snapshot.kind {
            DialogKind::CatalogImport => {
                if snapshot.draft.target.is_some() { return Err("New asset import requires a new choice draft; use explicit source replacement for an existing asset".into()); }
                let (asset,entry)=snapshot.draft.values()?;
                p.import_catalog_asset(asset,entry,path)?
            },
            DialogKind::CatalogReplace => {
                let id=snapshot.selection.as_deref().ok_or("Select a catalog entry first")?;
                let entry=p.catalog().and_then(|s|s.entries.iter().find(|e|e.id==id)).ok_or("Selected entry is no longer present")?;
                let asset=entry.asset.clone();
                if p.catalog().and_then(|s|s.assets.iter().find(|a|a.id==asset))!=snapshot.asset.as_ref() {
                    return Err("Selected catalog asset changed while chooser was open; no replacement applied".into());
                }
                p.replace_catalog_source(&asset,path)?
            },
            _=>return Err("Not a catalog chooser action".into()),
        };
        let id=if snapshot.kind==DialogKind::CatalogImport {snapshot.draft.id.clone()}else{snapshot.selection.unwrap()};
        Self::inspect_catalog_entry(&theme,&id)?;
        self.project=Some(candidate);
        self.catalog_committed(theme,Some(id));
        if snapshot.kind==DialogKind::CatalogReplace {
            self.catalog_draft=snapshot.draft;
            self.notice="Selected asset source replaced; unapplied draft metadata is retained. Apply explicitly to change metadata; unsaved changes.".into();
        }
        Ok(())
    }
    fn catalog_committed(&mut self, theme:crate::theme::Theme, selection:Option<String>) {
        self.preview=Some(theme); self.revision+=1; self.dirty=true; self.error=None;
        self.notice="Catalog transaction applied. Unsaved changes; Save NEW copy and Export NEW pack are separate actions.".into();
        self.catalog_selection=selection;
        self.load_catalog_draft();
    }
    pub(super) fn reset_catalog_context(&mut self) {
        self.catalog_selection=None; self.catalog_draft=Draft::default(); self.view_revision+=1;
    }
    fn load_catalog_draft(&mut self) {
        self.catalog_draft=Draft::default();
        if let Some(e)=self.catalog_selection.as_deref().and_then(|id|self.project.as_ref()?.catalog()?.entries.iter().find(|e|e.id==id)) {
            let mut d=Draft { target:Some(e.id.clone()), id:e.id.clone(), tag:e.tag.clone(), name:e.name.clone(), collision:e.defaults.collision, ..Draft::default() };
            if let Some(a)=self.project.as_ref().unwrap().catalog().unwrap().assets.iter().find(|a|a.id==e.asset) {d.use_asset(a);}
            self.catalog_draft=d;
        } else { self.catalog_selection=None; }
        self.view_revision+=1;
    }
    fn apply_catalog_draft(&mut self) -> Result<()> {
        let (asset,entry)=self.catalog_draft.values()?;
        let mut project=self.project.as_ref().ok_or("Start a pack first")?.clone();
        let mut candidate=project.catalog().cloned().unwrap_or_default();
        let current=candidate.assets.iter_mut().find(|a|a.id==asset.id).ok_or("Asset ID not retained; Import a new asset + choice, or choose a retained asset")?;
        if current.file!=asset.file {return Err("Local closure file cannot change on Apply; use Import or explicit source replacement".into());}
        *current=asset;
        if let Some(id)=&self.catalog_draft.target {
            if &entry.id!=id {return Err("Entry ID is stable; create a new choice for a different ID".into());}
            *candidate.entries.iter_mut().find(|e|&e.id==id).ok_or("Draft target is no longer present")?=entry.clone();
        } else {candidate.entries.push(entry.clone());}
        // An explicit rebind retires an asset only after its last reference is gone.
        candidate.assets.retain(|a|candidate.entries.iter().any(|e|e.asset==a.id));
        let theme=project.set_catalog(candidate)?;
        Self::inspect_catalog_entry(&theme,&entry.id)?;
        self.project=Some(project);
        self.catalog_committed(theme,Some(entry.id));
        Ok(())
    }
    fn remove_catalog_choice(&mut self) -> Result<()> {
        let id=self.catalog_selection.as_deref().ok_or("Select a catalog entry first")?;
        let theme=self.project.as_mut().ok_or("Start a pack first")?.remove_catalog_entry(id)?;
        self.catalog_committed(theme,None); Ok(())
    }

    pub(super) fn catalog_ui(&mut self, ui:&mut egui::Ui) {
        ui.label("Place Mode selectable choices — separate from the legacy host Decoration role.");
        ui.small("Selection and draft typing do not edit the Project. Apply / Import are explicit transactions. Fit edits affect every choice sharing that Asset ID.");
        let entries:Vec<_>=self.preview.as_ref().map(|t|t.decorations.entries.clone()).unwrap_or_default();
        egui::ScrollArea::vertical().id_salt("catalog-choices").max_height(130.).show(ui,|ui| {
            for (index,e) in entries.iter().enumerate() {
                let r=ui.push_id((&e.id,"catalog-choice"),|ui|ui.add(egui::Button::new(format!("{} — {}",e.id,e.name)).selected(self.catalog_selection.as_deref()==Some(&e.id)).wrap())).inner;
                self.controls.insert(ROWS[index],r.rect);
                if r.clicked() {
                    let check=self.preview.as_ref().ok_or_else(||"No loaded preview".to_owned()).and_then(|t|Self::inspect_catalog_entry(t,&e.id));
                    match check {
                        Ok(_)=>{self.catalog_selection=Some(e.id.clone());self.load_catalog_draft();self.camera.release();},
                        Err(e)=>self.error=Some(e),
                    }
                }
            }
        });
        if entries.is_empty() {ui.label("No selectable catalog choices.");}
        if self.button(ui,"catalog-new","New choice draft") {self.catalog_draft=Draft::default();}
        let assets=self.project.as_ref().and_then(|p|p.catalog()).map(|s|s.assets.clone()).unwrap_or_default();
        let combo=egui::ComboBox::from_id_salt("retained-catalog-asset").selected_text("Reuse retained asset…").show_ui(ui,|ui| {
            for (index,a) in assets.iter().enumerate() {
                let r=ui.selectable_label(false,format!("{} — {}",a.id,a.file));
                self.controls.insert(["catalog-reuse-0","catalog-reuse-1","catalog-reuse-2"][index],r.rect);
                if r.clicked() {self.catalog_draft.use_asset(a);}
            }
        });
        self.controls.insert("catalog-reuse",combo.response.rect);
        // Controls retain invalid numeric text until an explicit transaction reports it.
        for (id,label,value) in [
            ("catalog-entry-id","Stable Entry ID",&mut self.catalog_draft.id),
            ("catalog-tag","Semantic tag",&mut self.catalog_draft.tag),
            ("catalog-name","Display name",&mut self.catalog_draft.name),
            ("catalog-asset-id","Asset ID",&mut self.catalog_draft.asset),
            ("catalog-file","Local closure file (.glb leaf)",&mut self.catalog_draft.file),
            ("catalog-extent","Maximum source extent (0, 10] m",&mut self.catalog_draft.extent),
            ("catalog-scale","Scale [0.1, 4]",&mut self.catalog_draft.scale),
            ("catalog-yaw","Asset yaw [-180, 180] degrees",&mut self.catalog_draft.yaw),
            ("catalog-pivot-x","Pivot X [-3, 3]",&mut self.catalog_draft.pivot[0]),
        ] {
            ui.label(label); let r=ui.add(egui::TextEdit::singleline(value).id(egui::Id::unique(id)).desired_width(f32::INFINITY)); self.controls.insert(id,r.rect);
        }
        for (i,id,label) in [(1,"catalog-pivot-y","Pivot Y [-3, 3]"),(2,"catalog-pivot-z","Pivot Z [-3, 3]")] {
            ui.label(label);let r=ui.add(egui::TextEdit::singleline(&mut self.catalog_draft.pivot[i]).id(egui::Id::unique(id)).desired_width(f32::INFINITY));self.controls.insert(id,r.rect);
        }
        let r=ui.checkbox(&mut self.catalog_draft.collision,"Default decoration collision (metadata only)");self.controls.insert("catalog-collision",r.rect);
        if self.button(ui,"catalog-apply","Apply draft / reuse retained asset") && let Err(e)=self.apply_catalog_draft(){self.error=Some(e);}
        if self.button(ui,"catalog-import","Import NEW asset + choice GLB…") {self.request_dialog(DialogKind::CatalogImport);}
        ui.small("Choose source in the native panel; closure filename above is an explicit export name, not an original path. No implicit replacement.");
        if self.button(ui,"catalog-replace","Replace SELECTED entry's asset source…") {self.request_dialog(DialogKind::CatalogReplace);}
        if self.button(ui,"catalog-remove-entry","Remove SELECTED choice") && let Err(e)=self.remove_catalog_choice(){self.error=Some(e);}
        ui.small("Unreferenced assets retire automatically when their last choice is removed or rebound.");
        ui.small("Removing the final choice retains an explicit empty catalog: editable v4 / appearance v3, never legacy catalog absence.");
        egui::CollapsingHeader::new("Catalog limits, identity and inspection").show(ui,|ui| {
            ui.label("32 choices / 3 catalog assets. All roles + catalog share 3 model sources, 24 MB encoded, 300000 decoded vertices, 64 textures / 32 MB decoded textures, 64 MB retained payload. Each GLB ≤8 MB; editable manifest ≤16384 bytes; appearance manifest ≤32768 bytes. No extra Pin budget.");
            ui.label("IDs: 1–48 lowercase ASCII letters/digits/hyphens, initial letter, no trailing hyphen. Tags: 1–96 bytes, also dots, no consecutive/trailing dots. Names: nonblank ≤40 characters, no controls. Choices display in lexical Entry ID order.");
            ui.label("Exact retained asset reuse shares source authority; separately acquired equal bytes remain distinct. Sources must pass existing no-follow ancestor checks. Saved closure reopen follows established project-root rules. Reopened provenance names the saved copy, not a guaranteed original author-machine path.");
            ui.label("Selected mesh inspection uses the loaded visitor catalog and its grounded 1.6 m envelope contract, on a transient isolated stage. Visitor placement permits at most 32766 vertices per model; unplaceable candidates are refused without replacing last-good state. Visitor fit normalizes scale/pivot to the placement envelope; source extent admission and asset yaw remain shared. Inspection never creates .hair-layout, moves originals, changes exported roles or grants host/file collision.");
        });
    }
    /// Shared loaded-catalog placement, transient and storage-free. Called on
    /// selection/revision changes by the compositor, never on ordinary redraw.
    // Preserve the compositor's existing optional mesh/texture return contract.
    #[allow(clippy::type_complexity)]
    pub fn catalog_inspection(&self) -> Result<Option<(Vec<crate::assets::Vertex>, Vec<std::sync::Arc<crate::assets::Texture>>)>> {
        if self.category!="catalog" {return Ok(None);}
        let Some(theme)=&self.preview else {return Ok(None)};
        let Some(entry)=self.catalog_selection.as_deref().and_then(|id|theme.decorations.entry(id)) else {return Ok(None)};
        let mesh=Self::inspect_catalog_entry(theme,&entry.id)?;
        let textures=theme.decorations.asset(&entry.asset).ok_or("Selected catalog asset missing")?.model.textures.clone();
        Ok(Some((mesh,textures)))
    }
    fn inspect_catalog_entry(theme:&crate::theme::Theme,id:&str) -> Result<Vec<crate::assets::Vertex>> {
        let entry=theme.decorations.entry(id).ok_or("Selected catalog entry missing")?;
        let d=crate::placement_store::Decoration::from_entry(entry,[0.,0.],0.,[1.6;3])?;
        theme.decoration_preview(&d).map_err(|e|format!("Selected catalog mesh inspection refused: {e}"))
    }
}
const ROWS:[&str;32]=["catalog-row-0","catalog-row-1","catalog-row-2","catalog-row-3","catalog-row-4","catalog-row-5","catalog-row-6","catalog-row-7","catalog-row-8","catalog-row-9","catalog-row-10","catalog-row-11","catalog-row-12","catalog-row-13","catalog-row-14","catalog-row-15","catalog-row-16","catalog-row-17","catalog-row-18","catalog-row-19","catalog-row-20","catalog-row-21","catalog-row-22","catalog-row-23","catalog-row-24","catalog-row-25","catalog-row-26","catalog-row-27","catalog-row-28","catalog-row-29","catalog-row-30","catalog-row-31"];
