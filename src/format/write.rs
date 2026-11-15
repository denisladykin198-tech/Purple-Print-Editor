//! Сериализация `ProjectDoc` в MPP.

use crate::constants::ALLOWED_GRID_SIZES;
use crate::model::ProjectDoc;

use super::io::W;

pub fn write_mpp(doc: &ProjectDoc) -> Result<Vec<u8>, String> {
    let mut w = W(Vec::new());
    w.0.extend_from_slice(b"MPP");
    w.u16(2);

    let m = &doc.meta;
    w.str(&m.id);
    w.str(&m.name);
    w.str(&m.category);
    w.str(&m.subcategory);
    w.str(&m.description);
    w.u16(m.tags.len() as u16);
    for t in &m.tags {
        w.str(t);
    }
    w.f3(m.base_color);

    w.u32(doc.next_frame_id);
    w.u32(doc.next_socket_id);
    w.u32(doc.frames.len() as u32);

    for f in &doc.frames {
        if !ALLOWED_GRID_SIZES.contains(&f.size) {
            return Err(format!("Фрейм «{}»: недопустимый размер {}", f.name, f.size));
        }
        if !f.data_ok() {
            return Err(format!(
                "Фрейм «{}»: данные не синхронизированы (сначала сохрани активный фрейм)",
                f.name
            ));
        }

        w.u32(f.id);
        w.str(&f.name);
        w.str(&f.tag);
        w.u8(f.is_root as u8);
        w.u32(f.size);
        w.f3(f.pivot);
        w.str(&f.script);

        w.rle(&f.voxels);
        w.rle(&f.front);
        w.rle(&f.top);
        w.rle(&f.side);

        w.u32(f.sockets.len() as u32);
        for s in &f.sockets {
            w.u32(s.id);
            w.str(&s.name);
            w.str(&s.tag);
            w.f3(s.anchor);
            for r in s.rot {
                w.u8(r);
            }
            w.u8(s.required as u8);
            w.u32(s.selected as u32);
            // zone_id: 0 = нет привязки к скриптовой зоне.
            w.u64(s.zone_id.unwrap_or(0));

            w.u32(s.options.len() as u32);
            for o in &s.options {
                w.i32(o.frame_id);
                w.str(&o.label);
            }
        }

        w.u32(f.zones.len() as u32);
        for z in &f.zones {
            w.u64(z.id);
            w.str(&z.name);
            w.u8(z.axis);
            for v in z.min {
                w.u32(v);
            }
            for v in z.max {
                w.u32(v);
            }
            w.u32(z.color_idx);
            w.str(&z.script);
        }
    }

    Ok(w.0)
}