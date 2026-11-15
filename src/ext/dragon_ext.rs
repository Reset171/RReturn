use super::*;

fn pack_vec3(v: Vec3) -> Tag {
    Tag::List(vec![
        Tag::Double(v.x.to_bits()),
        Tag::Double(v.y.to_bits()),
        Tag::Double(v.z.to_bits()),
    ])
}

fn unpack_vec3(tag: Option<&Tag>) -> Option<Vec3> {
    let Tag::List(list) = tag? else {
        return None;
    };
    if list.len() != 3 {
        return None;
    }
    let get_f64 = |t: &Tag| match t {
        Tag::Double(bits) => Some(f64::from_bits(*bits)),
        _ => None,
    };
    Some(Vec3::new(
        get_f64(&list[0])?,
        get_f64(&list[1])?,
        get_f64(&list[2])?,
    ))
}

fn get_f64(tag: Option<&Tag>) -> Option<f64> {
    match tag {
        Some(Tag::Double(bits)) => Some(f64::from_bits(*bits)),
        _ => None,
    }
}

fn get_f32(tag: Option<&Tag>) -> Option<f32> {
    match tag {
        Some(Tag::Float(bits)) => Some(f32::from_bits(*bits)),
        _ => None,
    }
}

fn get_i32(tag: Option<&Tag>) -> Option<i32> {
    nbt_value::number(tag).map(|v| v as i32)
}

fn get_i64(tag: Option<&Tag>) -> Option<i64> {
    match tag {
        Some(Tag::Long(v)) => Some(*v),
        _ => None,
    }
}

fn get_bool(tag: Option<&Tag>) -> Option<bool> {
    match tag {
        Some(Tag::Byte(b)) => Some(*b != 0),
        _ => None,
    }
}

fn pack_who(who: FightWho) -> Tag {
    match who {
        FightWho::Host => Tag::IntArray(vec![0, 0]),
        FightWho::Guest(conn) => Tag::IntArray(vec![1, conn as i32]),
    }
}

fn unpack_who(tag: Option<&Tag>) -> Option<FightWho> {
    match tag {
        Some(Tag::IntArray(v)) if v.len() == 2 => match v[0] {
            0 => Some(FightWho::Host),
            1 => Some(FightWho::Guest(v[1] as u32)),
            _ => None,
        },
        _ => None,
    }
}

fn pack_path(path: &Option<PathBox>) -> Tag {
    match path {
        Some(p) => Tag::compound([
            ("next", Tag::Int(p.0.next as i32)),
            (
                "nodes",
                Tag::IntArray(p.0.nodes.iter().flat_map(|n| *n).collect()),
            ),
        ]),
        None => Tag::End,
    }
}

fn unpack_path(tag: Option<&Tag>) -> Option<PathBox> {
    let tag = tag?;
    if matches!(tag, Tag::End) {
        return None;
    }
    let next = get_i32(tag.get("next")).unwrap_or(0).max(0) as usize;
    let nodes = match tag.get("nodes") {
        Some(Tag::IntArray(arr)) => arr
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect(),
        _ => Vec::new(),
    };
    Some(PathBox(Path { nodes, next }))
}

fn static_damage_type(name: &str) -> &'static str {
    match name {
        "minecraft:player_attack" => "minecraft:player_attack",
        "minecraft:mace_smash" => "minecraft:mace_smash",
        "minecraft:spear" => "minecraft:spear",
        "minecraft:mob_attack" => "minecraft:mob_attack",
        "minecraft:arrow" => "minecraft:arrow",
        "minecraft:thrown" => "minecraft:thrown",
        "minecraft:fireball" => "minecraft:fireball",
        "minecraft:player_explosion" => "minecraft:player_explosion",
        "minecraft:explosion" => "minecraft:explosion",
        "minecraft:fall" => "minecraft:fall",
        "minecraft:fly_into_wall" => "minecraft:fly_into_wall",
        "minecraft:falling_block" => "minecraft:falling_block",
        "minecraft:falling_stalactite" => "minecraft:falling_stalactite",
        "minecraft:on_fire" => "minecraft:on_fire",
        "minecraft:in_fire" => "minecraft:in_fire",
        "minecraft:campfire" => "minecraft:campfire",
        "minecraft:lava" => "minecraft:lava",
        "minecraft:drown" => "minecraft:drown",
        "minecraft:dry_out" => "minecraft:dry_out",
        "minecraft:magic" => "minecraft:magic",
        "minecraft:indirect_magic" => "minecraft:indirect_magic",
        "minecraft:sting" => "minecraft:sting",
        "minecraft:cactus" => "minecraft:cactus",
        "minecraft:sweet_berry_bush" => "minecraft:sweet_berry_bush",
        "minecraft:in_wall" => "minecraft:in_wall",
        "minecraft:thorns" => "minecraft:thorns",
        "minecraft:sonic_boom" => "minecraft:sonic_boom",
        "minecraft:freeze" => "minecraft:freeze",
        "minecraft:out_of_world" => "minecraft:out_of_world",
        "minecraft:cramming" => "minecraft:cramming",
        "minecraft:falling_anvil" => "minecraft:falling_anvil",
        "minecraft:fireworks" => "minecraft:fireworks",
        "minecraft:generic_kill" => "minecraft:generic_kill",
        "minecraft:lightning_bolt" => "minecraft:lightning_bolt",
        "minecraft:outside_border" => "minecraft:outside_border",
        "minecraft:spit" => "minecraft:spit",
        "minecraft:stalagmite" => "minecraft:stalagmite",
        "minecraft:starve" => "minecraft:starve",
        "minecraft:hot_floor" => "minecraft:hot_floor",
        "minecraft:sulfur_cube_hot" => "minecraft:sulfur_cube_hot",
        "minecraft:wind_charge" => "minecraft:wind_charge",
        "minecraft:wither" => "minecraft:wither",
        "minecraft:wither_skull" => "minecraft:wither_skull",
        "minecraft:trident" => "minecraft:trident",
        _ => "minecraft:generic",
    }
}

fn pack_phase(phase: &Phase) -> Tag {
    let mut fields = vec![("id", Tag::Int(phase.id()))];
    match phase {
        Phase::Holding {
            path,
            target,
            clockwise,
        } => {
            fields.push(("path", pack_path(path)));
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
            fields.push(("clockwise", Tag::Byte(i8::from(*clockwise))));
        }
        Phase::Strafe {
            charge,
            path,
            target,
            clockwise,
            player,
        } => {
            fields.push(("charge", Tag::Int(*charge)));
            fields.push(("path", pack_path(path)));
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
            fields.push(("clockwise", Tag::Byte(i8::from(*clockwise))));
            if let Some(who) = player {
                fields.push(("player", pack_who(*who)));
            }
        }
        Phase::LandingApproach { path, target } => {
            fields.push(("path", pack_path(path)));
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
        }
        Phase::Landing { target } | Phase::Hover { target } => {
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
        }
        Phase::Takeoff {
            first,
            path,
            target,
        } => {
            fields.push(("first", Tag::Byte(i8::from(*first))));
            fields.push(("path", pack_path(path)));
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
        }
        Phase::Scanning { time } => {
            fields.push(("time", Tag::Int(*time)));
        }
        Phase::Attacking { ticks } | Phase::Flaming { ticks } => {
            fields.push(("ticks", Tag::Int(*ticks)));
        }
        Phase::Charging { target, since } => {
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
            fields.push(("since", Tag::Int(*since)));
        }
        Phase::Dying { target, time } => {
            if let Some(t) = target {
                fields.push(("target", pack_vec3(*t)));
            }
            fields.push(("time", Tag::Int(*time)));
        }
    }
    Tag::compound(fields)
}

fn unpack_phase(tag: Option<&Tag>) -> Phase {
    let Some(tag) = tag else {
        return Phase::Hover { target: None };
    };
    let id = get_i32(tag.get("id")).unwrap_or(10);
    let path = unpack_path(tag.get("path"));
    let target = unpack_vec3(tag.get("target"));
    let clockwise = get_bool(tag.get("clockwise")).unwrap_or(false);
    match id {
        0 => Phase::Holding {
            path,
            target,
            clockwise,
        },
        1 => Phase::Strafe {
            charge: get_i32(tag.get("charge")).unwrap_or(0),
            path,
            target,
            clockwise,
            player: unpack_who(tag.get("player")),
        },
        2 => Phase::LandingApproach { path, target },
        3 => Phase::Landing { target },
        4 => Phase::Takeoff {
            first: get_bool(tag.get("first")).unwrap_or(true),
            path,
            target,
        },
        5 => Phase::Flaming {
            ticks: get_i32(tag.get("ticks")).unwrap_or(0),
        },
        6 => Phase::Scanning {
            time: get_i32(tag.get("time")).unwrap_or(0),
        },
        7 => Phase::Attacking {
            ticks: get_i32(tag.get("ticks")).unwrap_or(0),
        },
        8 => Phase::Charging {
            target,
            since: get_i32(tag.get("since")).unwrap_or(0),
        },
        9 => Phase::Dying {
            target,
            time: get_i32(tag.get("time")).unwrap_or(0),
        },
        _ => Phase::Hover { target },
    }
}

impl Dragon {
    pub fn rreturn_pack(&self) -> Tag {
        let history_y: Vec<Tag> = self
            .history
            .iter()
            .map(|(y, _)| Tag::Double(y.to_bits()))
            .collect();
        let history_yaw: Vec<Tag> = self
            .history
            .iter()
            .map(|(_, yaw)| Tag::Float(yaw.to_bits()))
            .collect();
        let parts_cur: Vec<Tag> = self.parts.iter().map(|(cur, _)| pack_vec3(*cur)).collect();
        let parts_prev: Vec<Tag> = self.parts.iter().map(|(_, prev)| pack_vec3(*prev)).collect();
        let mut fields = vec![
            ("id", Tag::Int(self.id as i32)),
            ("position", pack_vec3(self.position)),
            ("previous", pack_vec3(self.previous)),
            ("velocity", pack_vec3(self.velocity)),
            ("yaw", Tag::Float(self.yaw.to_bits())),
            ("previous_yaw", Tag::Float(self.previous_yaw.to_bits())),
            ("yaw_accel", Tag::Float(self.yaw_accel.to_bits())),
            ("health", Tag::Float(self.health.to_bits())),
            ("hurt_time", Tag::Int(self.hurt_time)),
            ("invulnerable", Tag::Int(self.invulnerable)),
            ("last_hurt", Tag::Float(self.last_hurt.to_bits())),
            ("death_time", Tag::Int(self.death_time)),
            ("flap", Tag::Float(self.flap.to_bits())),
            ("old_flap", Tag::Float(self.old_flap.to_bits())),
            ("in_wall", Tag::Byte(i8::from(self.in_wall))),
            ("phase", pack_phase(&self.phase)),
            ("flame_count", Tag::Int(self.flame_count)),
            ("sitting_damage", Tag::Float(self.sitting_damage.to_bits())),
            ("growl", Tag::Int(self.growl)),
            ("ambient_time", Tag::Int(self.ambient_time)),
            ("ticks", Tag::Int(self.ticks)),
            ("history_y", Tag::List(history_y)),
            ("history_yaw", Tag::List(history_yaw)),
            ("head", Tag::Int(self.head as i32)),
            ("parts_cur", Tag::List(parts_cur)),
            ("parts_prev", Tag::List(parts_prev)),
            ("no_ai", Tag::Byte(i8::from(self.no_ai))),
            ("rng", Tag::Long(self.random.state() as i64)),
        ];
        if let Some((who, hit_tick)) = self.hurt_by_player {
            fields.push(("hurt_by_who", pack_who(who)));
            fields.push(("hurt_by_tick", Tag::Int(hit_tick)));
        }
        if let Some((ty, causing, direct)) = self.last_source {
            fields.push(("last_source_ty", Tag::String(ty.into())));
            if let Some(c) = causing {
                fields.push(("last_source_causing", Tag::Int(c)));
            }
            if let Some(d) = direct {
                fields.push(("last_source_direct", Tag::Int(d)));
            }
        }
        if let Some(nodes) = &self.nodes {
            fields.push((
                "nodes",
                Tag::IntArray(nodes.iter().flat_map(|n| *n).collect()),
            ));
        }
        if let Some(nc) = self.nearest_crystal {
            fields.push(("nearest_crystal", Tag::Int(nc as i32)));
        }
        if let Some(req) = self.strafe_request {
            fields.push(("strafe_request", pack_who(req)));
        }
        Tag::compound(fields)
    }

    pub fn rreturn_unpack(tag: &Tag, seed: i64) -> Option<Self> {
        let position = unpack_vec3(tag.get("position"))?;
        let yaw = get_f32(tag.get("yaw")).unwrap_or(0.0);
        let mut d = Self::new(position, yaw, seed);
        d.id = get_i32(tag.get("id")).unwrap_or(0) as u32;
        if let Some(prev) = unpack_vec3(tag.get("previous")) {
            d.previous = prev;
        }
        if let Some(vel) = unpack_vec3(tag.get("velocity")) {
            d.velocity = vel;
        }
        d.previous_yaw = get_f32(tag.get("previous_yaw")).unwrap_or(yaw);
        d.yaw_accel = get_f32(tag.get("yaw_accel")).unwrap_or(0.0);
        d.health = get_f32(tag.get("health")).unwrap_or(MAX_HEALTH);
        d.hurt_time = get_i32(tag.get("hurt_time")).unwrap_or(0);
        d.invulnerable = get_i32(tag.get("invulnerable")).unwrap_or(0);
        d.last_hurt = get_f32(tag.get("last_hurt")).unwrap_or(0.0);
        d.hurt_by_player = unpack_who(tag.get("hurt_by_who"))
            .zip(get_i32(tag.get("hurt_by_tick")));
        if let Some(Tag::String(s)) = tag.get("last_source_ty")
            && let Ok(name) = s.to_string()
        {
            d.last_source = Some((
                static_damage_type(&name),
                get_i32(tag.get("last_source_causing")),
                get_i32(tag.get("last_source_direct")),
            ));
        }
        d.death_time = get_i32(tag.get("death_time")).unwrap_or(0);
        d.flap = get_f32(tag.get("flap")).unwrap_or(0.0);
        d.old_flap = get_f32(tag.get("old_flap")).unwrap_or(0.0);
        d.in_wall = get_bool(tag.get("in_wall")).unwrap_or(false);
        d.phase = unpack_phase(tag.get("phase"));
        d.flame_count = get_i32(tag.get("flame_count")).unwrap_or(0);
        d.sitting_damage = get_f32(tag.get("sitting_damage")).unwrap_or(0.0);
        d.growl = get_i32(tag.get("growl")).unwrap_or(100);
        d.ambient_time = get_i32(tag.get("ambient_time")).unwrap_or(0);
        d.ticks = get_i32(tag.get("ticks")).unwrap_or(0);
        if let (Some(Tag::List(hy)), Some(Tag::List(hyaw))) =
            (tag.get("history_y"), tag.get("history_yaw"))
        {
            for i in 0..64.min(hy.len()).min(hyaw.len()) {
                if let (Some(y), Some(yw)) = (get_f64(Some(&hy[i])), get_f32(Some(&hyaw[i]))) {
                    d.history[i] = (y, yw);
                }
            }
        }
        d.head = (get_i32(tag.get("head")).unwrap_or(0).max(0) as usize) & 63;
        if let (Some(Tag::List(pc)), Some(Tag::List(pp))) =
            (tag.get("parts_cur"), tag.get("parts_prev"))
        {
            for i in 0..8.min(pc.len()).min(pp.len()) {
                if let (Some(cur), Some(prev)) = (unpack_vec3(Some(&pc[i])), unpack_vec3(Some(&pp[i])))
                {
                    d.parts[i] = (cur, prev);
                }
            }
        }
        if let Some(Tag::IntArray(arr)) = tag.get("nodes")
            && arr.len() == 72
        {
            let mut nodes = [[0i32; 3]; 24];
            for (i, c) in arr.as_chunks::<3>().0.iter().enumerate() {
                nodes[i] = *c;
            }
            d.nodes = Some(nodes);
        }
        d.nearest_crystal = get_i32(tag.get("nearest_crystal")).map(|v| v.max(0) as usize);
        d.strafe_request = unpack_who(tag.get("strafe_request"));
        d.no_ai = get_bool(tag.get("no_ai")).unwrap_or(false);
        if let Some(st) = get_i64(tag.get("rng")) {
            d.random = LegacyRandom::from_state(st as u64);
        }
        Some(d)
    }
}

impl Fight {
    pub fn rreturn_pack(&self) -> Tag {
        let crystals: Vec<Tag> = self.respawn_crystals.iter().map(|v| pack_vec3(*v)).collect();
        let mut fields = vec![
            ("loaded", Tag::Byte(i8::from(self.loaded))),
            ("needs_scan", Tag::Byte(i8::from(self.needs_scan))),
            ("killed", Tag::Byte(i8::from(self.killed))),
            ("previously_killed", Tag::Byte(i8::from(self.previously_killed))),
            ("dragon_unseen", Tag::Int(self.dragon_unseen)),
            ("gateways", Tag::IntArray(self.gateways.clone())),
            ("alive_crystals", Tag::Int(self.alive_crystals)),
            ("crystal_scan", Tag::Int(self.crystal_scan)),
            ("player_scan", Tag::Int(self.player_scan)),
            ("players", Tag::Byte(i8::from(self.players))),
            ("crystals_placed", Tag::Byte(i8::from(self.crystals_placed))),
            ("respawn_crystals", Tag::List(crystals)),
            ("dirty", Tag::Byte(i8::from(self.dirty))),
        ];
        if let Some(uuid) = self.dragon_uuid {
            fields.push(("dragon_uuid", Tag::IntArray(uuid.to_vec())));
        }
        if let Some(ep) = self.exit_portal {
            fields.push(("exit_portal", Tag::IntArray(ep.to_vec())));
        }
        if let Some((stage, time)) = self.respawn {
            fields.push(("respawn_stage", Tag::String(stage.name().into())));
            fields.push(("respawn_time", Tag::Int(time)));
        }
        if let Some(who) = self.blast_by {
            fields.push(("blast_by", pack_who(who)));
        }
        Tag::compound(fields)
    }

    pub fn rreturn_unpack(tag: &Tag) -> Self {
        let mut f = Self::default();
        f.loaded = get_bool(tag.get("loaded")).unwrap_or(false);
        f.needs_scan = get_bool(tag.get("needs_scan")).unwrap_or(false);
        f.killed = get_bool(tag.get("killed")).unwrap_or(false);
        f.previously_killed = get_bool(tag.get("previously_killed")).unwrap_or(false);
        f.dragon_uuid = match tag.get("dragon_uuid") {
            Some(Tag::IntArray(v)) if v.len() == 4 => Some([v[0], v[1], v[2], v[3]]),
            _ => None,
        };
        f.dragon_unseen = get_i32(tag.get("dragon_unseen")).unwrap_or(0);
        if let Some(Tag::IntArray(v)) = tag.get("gateways") {
            f.gateways = v.clone();
        }
        f.exit_portal = match tag.get("exit_portal") {
            Some(Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
            _ => None,
        };
        f.alive_crystals = get_i32(tag.get("alive_crystals")).unwrap_or(0);
        f.crystal_scan = get_i32(tag.get("crystal_scan")).unwrap_or(0);
        f.player_scan = get_i32(tag.get("player_scan")).unwrap_or(0);
        f.players = get_bool(tag.get("players")).unwrap_or(false);
        f.crystals_placed = get_bool(tag.get("crystals_placed")).unwrap_or(false);
        if let Some(Tag::String(s)) = tag.get("respawn_stage")
            && let Ok(name) = s.to_string()
        {
            f.respawn = Respawn::from_name(&name).map(|stage| (stage, get_i32(tag.get("respawn_time")).unwrap_or(0)));
        }
        if let Some(Tag::List(list)) = tag.get("respawn_crystals") {
            f.respawn_crystals = list
                .iter()
                .filter_map(|t| unpack_vec3(Some(t)))
                .collect();
        }
        f.blast_by = unpack_who(tag.get("blast_by"));
        f.last_blow = None;
        f.dirty = get_bool(tag.get("dirty")).unwrap_or(false);
        f
    }
}