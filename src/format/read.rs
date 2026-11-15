//! Загрузка MPP.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::constants::{ALLOWED_GRID_SIZES, NO_FRAME};
use crate::model::{
    Frame, ProjectDoc, ProjectMeta, Socket, SocketOption, ZoneRec,
};

use super::io::{R, FRAME_LIMIT, OPTION_LIMIT, SOCKET_LIMIT, ZONE_LIMIT};

pub fn read_mpp(bytes: &[u8]) -> Result<ProjectDoc, String> {
    let mut r = R { d: bytes, p: 0 };
    let magic = r.take(3)?;

    if magic != b"MPP" {
        return Err("Неверный формат файла (ожидается MPP)".into());
    }

    let ver = r.u16()?;
    if ver != 2 {
        return Err(format!("Неподдерживаемая версия MPP: {}", ver));
    }

    let id = r.str()?;
    let name = r.str()?;
    let category = r.str()?;
    let subcategory = r.str()?;
    let description = r.str()?;

    let tag_n = r.u16()? as usize;
    let mut tags = Vec::with_capacity(tag_n);
    for _ in 0..tag_n {
        tags.push(r.str()?);
    }

    let base_color = r.f3()?;
    let mut next_frame_id = r.u32()?;
    let mut next_socket_id = r.u32()?;

    let frame_n = r.count(FRAME_LIMIT, "фреймов")?;
    if frame_n == 0 {
        return Err("В файле нет фреймов".into());
    }

    let mut frames: Vec<Frame> = Vec::with_capacity(frame_n);

    for _ in 0..frame_n {
        let fid = r.u32()?;
        let fname = r.str()?;
        let ftag = r.str()?;
        let is_root = r.u8()? != 0;
        let size = r.u32()?;
        if !ALLOWED_GRID_SIZES.contains(&size) {
            return Err(format!("Неподдерживаемый размер сетки: {}", size));
        }
        let pivot = r.f3()?;
        let script = r.str()?;

        let n2 = (size as usize) * (size as usize);
        let voxels = r.rle(n2 * size as usize)?;
        let front = r.rle(n2)?;
        let top = r.rle(n2)?;
        let side = r.rle(n2)?;

        let sock_n = r.count(SOCKET_LIMIT, "слотов")?;
        let mut sockets = Vec::with_capacity(sock_n);

        for _ in 0..sock_n {
            let sid = r.u32()?;
            let sname = r.str()?;
            let stag = r.str()?;
            let anchor = r.f3()?;
            let rot = [r.u8()? % 4, r.u8()? % 4, r.u8()? % 4];
            let required = r.u8()? != 0;
            let selected = r.u32()? as usize;
            // 0 = слот без привязки к зоне.
            let zone_raw = r.u64()?;
            let zone_id = if zone_raw == 0 { None } else { Some(zone_raw) };

            let opt_n = r.count(OPTION_LIMIT, "вариантов")?;
            let mut options = Vec::with_capacity(opt_n);
            for _ in 0..opt_n {
                let frame_id = r.i32()?;
                let label = r.str()?;
                options.push(SocketOption { frame_id, label });
            }

            if options.is_empty() {
                options.push(SocketOption {
                    frame_id: NO_FRAME,
                    label: "Без модуля".into(),
                });
            }
            let selected = if selected < options.len() {
                selected
            } else {
                0
            };

            sockets.push(Socket {
                id: sid,
                name: sname,
                tag: stag,
                anchor,
                rot,
                required,
                options,
                selected,
                zone_id,
            });
        }

        let zone_n = r.count(ZONE_LIMIT, "зон")?;
        let mut zones = Vec::with_capacity(zone_n);
        for _ in 0..zone_n {
            let zid = r.u64()?;
            let zname = r.str()?;
            let axis = r.u8()?;
            if axis > 2 {
                return Err(format!("Зона «{}»: недопустимая ось {}", zname, axis));
            }
            let min = [r.u32()?, r.u32()?, r.u32()?];
            let max = [r.u32()?, r.u32()?, r.u32()?];
            let color_idx = r.u32()?;
            let zscript = r.str()?;
            zones.push(ZoneRec {
                id: zid,
                name: zname,
                axis,
                min,
                max,
                color_idx,
                script: zscript,
            });
        }

        frames.push(Frame {
            id: fid,
            name: fname,
            tag: ftag,
            is_root,
            size,
            pivot,
            voxels,
            front,
            top,
            side,
            sockets,
            zones,
            script,
            revision: 1,
            voxel_count_cache: Arc::new(Mutex::new(None)),
        });
    }

    if !frames[0].is_root {
        return Err("Первый фрейм в файле не корневой".into());
    }

    // --- Валидация целостности ---
    let mut seen_frames = HashSet::new();
    for f in &frames {
        if !seen_frames.insert(f.id) {
            return Err(format!("Дубликат id фрейма: {}", f.id));
        }
    }

    let max_fid = frames.iter().map(|f| f.id).max().unwrap_or(0);
    let max_sid = frames
        .iter()
        .flat_map(|f| f.sockets.iter())
        .map(|s| s.id)
        .max()
        .unwrap_or(0);

    // Автокоррекция next_* — защита от «залипших» счётчиков после
    // ручного редактирования файла.
    next_frame_id = next_frame_id.max(max_fid.saturating_add(1));
    next_socket_id = next_socket_id.max(max_sid.saturating_add(1));

    Ok(ProjectDoc {
        meta: ProjectMeta {
            id,
            name,
            category,
            subcategory,
            description,
            tags,
            base_color,
        },
        frames,
        next_frame_id,
        next_socket_id,
    })
}