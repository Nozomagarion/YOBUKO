// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `automation.*` commands; what runs them is in the modules they name.

use super::*;

const RANGE_START: Param = param("start", ParamType::Number, "the range's first arrangement beat, from 0");
const RANGE_END: Param = param("end", ParamType::Number, "the range's end, an arrangement beat after start");

pub(super) static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "automation.addPoint",
        label: "Add a point to a parameter's automation (envelope) over the arrangement, making the lane if it has none; the value moves from point to point along each segment's curve",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            param("beat", ParamType::Number, "arrangement beat, from 0; a second point at one beat makes a jump"),
            param(
                "value",
                ParamType::AutomationValue,
                "value in the unit the parameter's set command takes: volume in dB (at most +6; below -150 is silence), pan -1 to 1, mute 0 or 1, a device's as device.setParam",
            ),
            CURVE,
            optional(
                "shape",
                ParamType::Text,
                "a shape for the segment from the point before to the new one, between their heights, as automation.insertShape names them (a repeating one once a beat)",
            ),
        ],
        kind: CommandKind::Edit,
        run: crate::automation::add_point,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.movePoint",
        label: "Move a point of a parameter's automation to another beat or value, or change its curve",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            POINT,
            optional("beat", ParamType::Number, "new arrangement beat"),
            optional("value", ParamType::AutomationValue, "new value, as for automation.addPoint"),
            CURVE,
        ],
        kind: CommandKind::Edit,
        run: crate::automation::move_point,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.deletePoint",
        label: "Delete a point of a parameter's automation; the lane goes with its last point",
        params: &[DEVICE_TRACK, AUTOMATED_DEVICE, AUTOMATED, POINT],
        kind: CommandKind::Edit,
        run: crate::automation::delete_point,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.movePoints",
        label: "Move several points of one parameter's automation together, by a number of beats and by a value, in one undo step; values stay in the parameter's range",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            param(
                "points",
                ParamType::Numbers,
                "point indices on the lane, from 0 in beat order, as in automation.list",
            ),
            optional(
                "beats",
                ParamType::Number,
                "beats to move the points by: later above 0, earlier below; they must stay from beat 0",
            ),
            optional(
                "value",
                ParamType::Number,
                "how much to change each value by, in the unit of automation.addPoint (dB for the volume, where silence stays silent)",
            ),
            optional("curve", ParamType::Number, "a curve to give every moved point's segment, -1 to 1"),
        ],
        kind: CommandKind::Edit,
        run: crate::automation::move_points,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.deletePoints",
        label: "Delete several points of one parameter's automation in one undo step; the lane goes with its last point",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            param(
                "points",
                ParamType::Numbers,
                "point indices on the lane, from 0 in beat order, as in automation.list",
            ),
        ],
        kind: CommandKind::Edit,
        run: crate::automation::delete_points,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.clear",
        label: "Remove a parameter's automation, or without param every lane of the track's mixer or of the device",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            optional("param", ParamType::Text, "the parameter, as for automation.addPoint; left out for every lane"),
        ],
        kind: CommandKind::Edit,
        run: crate::automation::clear,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.setEnabled",
        label: "Turn a parameter's automation off or on again; a manual change to an automated parameter turns it off, and the value set by hand holds until it is on again",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            param("enabled", ParamType::Bool, "false turns the lane off, true on"),
        ],
        kind: CommandKind::Edit,
        run: crate::automation::set_enabled,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.copy",
        label: "Copy a time range of a parameter's automation, its values at the range's edges with it, for automation.paste",
        params: &[DEVICE_TRACK, AUTOMATED_DEVICE, AUTOMATED, RANGE_START, RANGE_END],
        kind: CommandKind::Read,
        run: crate::automation_range::copy,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.paste",
        label: "Paste the copied automation range onto a parameter's lane from a beat, over what is there; on another parameter each value sits where it sat on its own lane (the same height); the envelope outside the range stays",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            param("start", ParamType::Number, "arrangement beat the pasted range starts at, from 0"),
        ],
        kind: CommandKind::Edit,
        run: crate::automation_range::paste,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.duplicate",
        label: "Copy a time range of a parameter's automation right after itself, over what is there; answers the copy's range; the clipboard stays as it was",
        params: &[DEVICE_TRACK, AUTOMATED_DEVICE, AUTOMATED, RANGE_START, RANGE_END],
        kind: CommandKind::Edit,
        run: crate::automation_range::duplicate,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.deleteRange",
        label: "Delete the points of a parameter's automation inside a time range; the values at the range's edges stay, so the envelope outside it does not change",
        params: &[DEVICE_TRACK, AUTOMATED_DEVICE, AUTOMATED, RANGE_START, RANGE_END],
        kind: CommandKind::Edit,
        run: crate::automation_range::delete_range,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.insertShape",
        label: "Fill a time range of a parameter's automation with a shape over the parameter's whole range, or with point the segment from the point before it to it, between the two points' heights: sine, triangle, saw_up, saw_down, square (these repeat), ramp_up, ramp_down, curve_up, curve_down or swell; the envelope outside stays",
        params: &[
            DEVICE_TRACK,
            AUTOMATED_DEVICE,
            AUTOMATED,
            optional("start", ParamType::Number, "the range's first arrangement beat, from 0 (with end; or point)"),
            optional("end", ParamType::Number, "the range's end, an arrangement beat after start"),
            optional(
                "point",
                ParamType::Integer,
                "instead of a range: the point, from 0, whose segment from the point before gets the shape (the first point's: to the next)",
            ),
            param(
                "shape",
                ParamType::Text,
                "sine, triangle, saw_up, saw_down, square, ramp_up, ramp_down, curve_up, curve_down or swell",
            ),
            optional(
                "cycles",
                ParamType::Integer,
                "how many times a repeating shape plays, 1 to 512 (default 1 in a range, once a beat on a point's segment)",
            ),
        ],
        kind: CommandKind::Edit,
        run: crate::automation_range::insert_shape,
        policy: Policy::DEFAULT,
    },
    CommandSpec {
        id: "automation.list",
        label: "List a track's automation: its mixer's lanes and each device's, with their points (beat, value, curve) and whether they play",
        params: &[DEVICE_TRACK],
        kind: CommandKind::Read,
        run: crate::automation::list,
        policy: Policy::DEFAULT,
    },
];
