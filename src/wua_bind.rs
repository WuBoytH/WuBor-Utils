use {
    smash::{
        lua2cpp::L2CFighterCommon,
        phx::Hash40,
        app::{lua_bind::*, *},
        lib::{lua_const::*, L2CValue}
    },
    smash_script::*,
    crate::{
        vars::*,
        table_const::*,
        cancels::*
    }
};

#[allow(non_snake_case)]
pub mod WarkModule {
    use super::*;

    /// A shortcut to reset i32 variables to 0.
    pub unsafe fn reset_i32(module_accessor: *mut BattleObjectModuleAccessor, flag: i32) {
        WorkModule::set_int(module_accessor, 0, flag);
    }

    /// A shortcut to reset f32 variables to 0.
    pub unsafe fn reset_f32(module_accessor: *mut BattleObjectModuleAccessor, flag: i32) {
        WorkModule::set_float(module_accessor, 0.0, flag);
    }

    /// A shortcut to add a value to an i32 variable.
    pub unsafe fn add_i32(module_accessor: *mut BattleObjectModuleAccessor, flag: i32, amount: i32) {
        let counter = WorkModule::get_int(module_accessor, flag) + amount;
        WorkModule::set_int(module_accessor, counter, flag);
    }

    /// A shortcut to add a value to an f32 variable.
    pub unsafe fn add_f32(module_accessor: *mut BattleObjectModuleAccessor, flag: i32, amount: f32) {
        let counter = WorkModule::get_float(module_accessor, flag) + amount;
        WorkModule::set_float(module_accessor, counter, flag);
    }

    /// A function for incrementing an f32 variable by an amount.
    /// This function takes into account the effects of slowdown, such as from
    /// Bayonett's Witch Time or from the Timer item.
    pub unsafe fn count_down(module_accessor: *mut BattleObjectModuleAccessor, flag: i32, amount: f32) {
        let slow_rate = SlowModule::rate(module_accessor);
        let global_slow_rate = sv_information::slow_rate();
        let counter = WorkModule::get_float(module_accessor, flag) - (amount * slow_rate * global_slow_rate);
        WorkModule::set_float(module_accessor, counter, flag);
    }

    pub unsafe fn is_operation_cpu(module_accessor: *mut BattleObjectModuleAccessor) -> bool {
        if utility::get_category(&mut *module_accessor) != *BATTLE_OBJECT_CATEGORY_FIGHTER {
            return false;
        }
        let entry_id = WorkModule::get_int(module_accessor, *FIGHTER_INSTANCE_WORK_ID_INT_ENTRY_ID) as i32;
        let fighterentryid = smash::app::FighterEntryID(entry_id);
        let fighterinformation = smash::app::lua_bind::FighterManager::get_fighter_information(singletons::FighterManager(), fighterentryid);
        smash::app::lua_bind::FighterInformation::is_operation_cpu(fighterinformation)
    }
}

#[allow(non_snake_case)]
pub mod FGCModule {
    use super::*;

    /// A function used to enable jump-cancels, styled after the special cancel functions that Ryu, Ken, and Terry use.
    pub unsafe fn jump_cancel_check_hit(fighter: &mut L2CFighterCommon, jump_on_block: bool) -> L2CValue {
        let cancel_timer = WorkModule::get_float(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLOAT_CANCEL_TIMER);
        if (AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_HIT)
        || (AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_SHIELD) && jump_on_block))
        && !AttackModule::is_infliction(fighter.module_accessor, *COLLISION_KIND_MASK_ALL)
        && !fighter.global_table[IN_HITLAG].get_bool()
        && cancel_timer > 0.0 {
            let sit = fighter.global_table[SITUATION_KIND].get_i32();
            jump_cancel_common(fighter, sit.into())
        }
        else {
            false.into()
        }
    }

    /// A function used to enable jump-cancels, but it forces the jump-cancel regardless of if you hit anything.
    pub unsafe fn jump_cancel_check_exception(fighter: &mut L2CFighterCommon) -> L2CValue {
        let sit = fighter.global_table[SITUATION_KIND].get_i32();
        jump_cancel_common(fighter, sit.into())
    }

    /// Used to enable dash-cancels. You need to specify whether you're cancelling into a forward dash (reverse = false) or back dash (reverse = true);
    pub unsafe fn dash_cancel_check(fighter: &mut L2CFighterCommon, dash_on_block: bool, reverse: bool) -> L2CValue {
        let dir;
        let cat;
        let status;
        let cancel_timer = WorkModule::get_float(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLOAT_CANCEL_TIMER);
        if reverse {
            dir = 4;
            cat = *FIGHTER_PAD_CMD_CAT1_FLAG_TURN_DASH;
            status = *FIGHTER_STATUS_KIND_TURN_DASH;
        }
        else {
            dir = 6;
            cat = *FIGHTER_PAD_CMD_CAT1_FLAG_DASH;
            status = *FIGHTER_STATUS_KIND_DASH;
        }
        if (AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_HIT)
        || (AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_SHIELD) && dash_on_block))
        && !AttackModule::is_infliction(fighter.module_accessor, *COLLISION_KIND_MASK_ALL)
        && !fighter.global_table[IN_HITLAG].get_bool()
        && cancel_timer > 0.0
        && ControlModule::get_command_flag_cat(fighter.module_accessor, 0) & cat != 0
        && get_command_stick_direction(fighter, true) == dir {
            StatusModule::change_status_request_from_script(fighter.module_accessor, status, true);
            return true.into();
        }
        false.into()
    }

    /// Enables cancels that would normally be impossible using the existing cancel system.
    ///
    /// # Arguments
    ///
    /// * `next_status` - The status kind you want to transition to (*FIGHTER_STATUS_KIND_XXXXXX)
    /// * `cat1_compare` - The cat1 flag you wish to check in order to transition to the next status (*FIGHTER_PAD_CMD_CAT1_FLAG_XXXXXX)
    ///
    /// # Example
    ///
    /// ```
    /// // Checks if you are in the Forward Tilt status, then checks if you input another Forward Tilt.
    /// // On hit, it will transition into Dash Attack.
    /// if StatusModule::status_kind(fighter.module_accessor) == *FIGHTER_STATUS_KIND_ATTACK_S3 {
    ///     FGCModule::cancel_exceptions(fighter, *FIGHTER_STATUS_KIND_ATTACK_DASH, *FIGHTER_PAD_CMD_CAT1_FLAG_ATTACK_S3, true);
    /// }
    /// ```
    pub unsafe fn cancel_exceptions(fighter: &mut L2CFighterCommon, next_status: i32, cat1_compare: i32, on_hit: bool) -> L2CValue {
        let cat1 = ControlModule::get_command_flag_cat(fighter.module_accessor, 0);
        let cancel_timer = WorkModule::get_float(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLOAT_CANCEL_TIMER);
        if !on_hit
        || ((AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_HIT)
        || AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_SHIELD))
        && !AttackModule::is_infliction(fighter.module_accessor, *COLLISION_KIND_MASK_ALL)
        && !fighter.global_table[IN_HITLAG].get_bool()
        && cancel_timer > 0.0) {
            if (cat1 & cat1_compare) != 0 {
                StatusModule::change_status_request_from_script(fighter.module_accessor, next_status, true);
                return true.into();
            }
        }
        false.into()
    }

    /// Used for moves that are able to cancel into themselves.
    /// # Arguments
    ///
    /// * `cat1_compare` - The cat1 flag you wish to check in order to transition to the next status (*FIGHTER_PAD_CMD_CAT1_FLAG_XXXXXX)
    /// * `counter` - The constant you're using to store the number of times you chain cancelled a move.
    /// * `max` - How many times you will be allowed to chain cancel a move before needed to go into normal endlag.
    ///
    /// # Example
    ///
    /// ```
    /// // Checks if you are in the Down Tilt status, then checks if you input another Down Tilt.
    /// // On hit, it will transition into Dash Attack.
    /// if StatusModule::status_kind(fighter.module_accessor) == *FIGHTER_STATUS_KIND_ATTACK_LW3 {
    ///     FGCModule::chain_cancels(fighter, *FIGHTER_STATUS_KIND_ATTACK_LW3, *FIGHTER_PAD_CMD_CAT1_FLAG_ATTACK_LW3, true, FIGHTER_DOLLY_INSTANCE_WORK_ID_INT_D_TILT_CHAIN_COUNT, 2);
    /// }
    /// ```
    pub unsafe fn chain_cancels(fighter: &mut L2CFighterCommon, cat1_compare: i32, on_hit: bool, counter: i32, max: i32) -> L2CValue {
        let cat1 = ControlModule::get_command_flag_cat(fighter.module_accessor, 0);
        let cancel_timer = WorkModule::get_float(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLOAT_CANCEL_TIMER);
        if !on_hit
        || ((AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_HIT)
        || AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_SHIELD))
        && !fighter.global_table[IN_HITLAG].get_bool()
        && cancel_timer > 0.0) {
            let count = WorkModule::get_int(fighter.module_accessor, counter) + 1;
            if (cat1 & cat1_compare) != 0
            && count <= max {
                fighter.attack_mtrans_pre_process();
                WorkModule::inc_int(fighter.module_accessor, counter);
                return 1.into();
            }
        }
        0.into()
    }

    /// The generic cancel system, used to enable special cancels for any character.
    /// # Arguments
    ///
    /// * `normal_cancels` - A vector of all of the ground attack transition terms you can cancel into.
    /// * `special_cancels` - A vector of all of the special move transition terms you can cancel into.
    /// * `aerial_cancel` - Checks if you can cancel into an aerial.
    /// * `jump_cancel` - Checks if you can jump-cancel. 0 = None | 1 = On Hit | 2 = On Hit or Block
    pub unsafe fn cancel_system(fighter: &mut L2CFighterCommon, normal_cancels: Vec<i32>, special_cancels: Vec<i32>, aerial_cancel: bool, jump_cancel: i32) {
        let cancel_timer = WorkModule::get_float(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLOAT_CANCEL_TIMER);
        if (AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_HIT)
        || AttackModule::is_infliction_status(fighter.module_accessor, *COLLISION_KIND_MASK_SHIELD))
        && !AttackModule::is_infliction(fighter.module_accessor, *COLLISION_KIND_MASK_ALL)
        && !fighter.global_table[IN_HITLAG].get_bool()
        && cancel_timer > 0.0 {
            if jump_cancel != 0
            && jump_cancel_check_hit(fighter, jump_cancel == 2).get_bool() {
                return;
            }
            let sit = fighter.global_table[SITUATION_KIND].get_i32();
            if special_cancels.is_empty() == false
            && special_cancel_common(fighter, sit.into(), special_cancels).get_bool() {
                return;
            }
            if aerial_cancel
            && sit == *SITUATION_KIND_AIR
            && aerial_cancel_common(fighter).get_bool() {
                return;
            }
            if normal_cancels.is_empty() == false
            && sit == *SITUATION_KIND_GROUND
            && normal_cancel_common(fighter, normal_cancels).get_bool() {
                return;
            }
        }
    }

    /// Checks the direction of the left stick and returns a number between 1 and 9, representing numpad notation.
    /// # Arguments
    /// 
    /// * `command` - Set to true to have the horizontal stick value reversed, so that it checks the input as if you are facing right.
    pub unsafe fn get_command_stick_direction(fighter: &mut L2CFighterCommon, command: bool) -> i32 {
        let status_kind = StatusModule::status_kind(fighter.module_accessor);
        let mut stick_x = fighter.global_table[STICK_X].get_f32();
        let stick_y = fighter.global_table[STICK_Y].get_f32();
        if command {
            stick_x = stick_x * PostureModule::lr(fighter.module_accessor);
            if status_kind == *FIGHTER_STATUS_KIND_TURN_RUN {
                stick_x *= -1.0;
            }
        }
        let length = sv_math::vec2_length(stick_x, stick_y);
        if length < 0.25 {
            return 5;
        }
        let normalize = sv_math::vec2_normalize(stick_x, stick_y);
        let dirx = normalize.x;
        let diry = normalize.y;
        let arctangent = diry.atan2(dirx.abs());
        let degrees = arctangent.to_degrees();
        if degrees.abs() <= 15.0 {
            if stick_x > 0.0 {
                return 6;
            }
            return 4;
        }
        else if 70.0 <= degrees.abs() {
            if stick_y > 0.0 {
                return 8;
            }
            return 2;
        }
        else {
            if stick_x > 0.0 {
                if stick_y > 0.0 {
                    return 9;
                }
                return 3;
            }
            if stick_y > 0.0 {
                return 7;
            }
            return 1;
        }
    }

    /// Used for command inputs. Currently goes unused.
    // pub unsafe fn inc_command(fighter: &mut L2CFighterCommon, flag: i32, timer_flag: i32) {
    //     WorkModule::inc_int(fighter.module_accessor, flag);
    //     WorkModule::set_int(fighter.module_accessor, 0, timer_flag);
    // }
    
    /// Checks the timer for a command input. Resets the command input if the timer exceeds the window, otherwise increments the timer by 1.
    // pub unsafe fn check_command_inc(fighter: &mut L2CFighterCommon, flag: i32, timer_flag: i32, window: i32) {
    //     if WorkModule::get_int(fighter.module_accessor, timer_flag) <= window {
    //         if WorkModule::get_int(fighter.module_accessor, flag) > 1 {
    //             WorkModule::inc_int(fighter.module_accessor, timer_flag);
    //         }
    //     }
    //     else {
    //         WarkModule::reset_i32(fighter.module_accessor, flag);
    //         WorkModule::set_int(fighter.module_accessor, 0, timer_flag);
    //     }
    // }

    /// Disables a grounded attack. Used for cancel systems with complex cancel trees.
    pub unsafe fn disable_ground_normal(fighter: &mut L2CFighterCommon, ground_normal_mask: i32) {
        if !CancelModule::is_enable_cancel(fighter.module_accessor) {
            let mut used_ground_normals = WorkModule::get_int(fighter.module_accessor, FIGHTER_INSTANCE_WORK_ID_INT_USED_GROUND_NORMALS);
            if used_ground_normals & ground_normal_mask == 0 {
                used_ground_normals += ground_normal_mask;
            }
            WorkModule::set_int(fighter.module_accessor, used_ground_normals, FIGHTER_INSTANCE_WORK_ID_INT_USED_GROUND_NORMALS);
        }
    }

    /// Used when checking for attack inputs to disable certain attacks if they are used in a string.
    /// Used for cancel systems with complex cancel trees.
    pub unsafe fn set_used_ground_normal_transition_terms(fighter: &mut L2CFighterCommon) {
        if !CancelModule::is_enable_cancel(fighter.module_accessor) {
            let used_mask = WorkModule::get_int(fighter.module_accessor, FIGHTER_INSTANCE_WORK_ID_INT_USED_GROUND_NORMALS);
            if used_mask & ATTACK_N_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SWING);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SHOOT);
            }
            if used_mask & ATTACK_S3_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_S3);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SWING_3);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SHOOT_S3);
            }
            if used_mask & ATTACK_HI3_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_HI3);
            }
            if used_mask & ATTACK_LW3_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_LW3);
            }
            if used_mask & ATTACK_S4_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_S4_START);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SWING_4);
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ITEM_SHOOT_S4);
            }
            if used_mask & ATTACK_HI4_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_HI4_START);
            }
            if used_mask & ATTACK_LW4_MASK != 0 {
                WorkModule::unable_transition_term(fighter.module_accessor, *FIGHTER_STATUS_TRANSITION_TERM_ID_CONT_ATTACK_LW4_START);
            }
        }
    }

    /// Resets your current cancel string. Typically only resets if CancelModule::is_enable_cancel is true, but
    /// the ignore flag can be passed to bypass that.
    pub unsafe fn reset_used_ground_normals(fighter: &mut L2CFighterCommon, ignore: bool) {
        if ignore || (CancelModule::is_enable_cancel(fighter.module_accessor)
        || MotionModule::is_end(fighter.module_accessor))
        || ![
            *FIGHTER_STATUS_KIND_ATTACK,
            *FIGHTER_STATUS_KIND_ATTACK_100,
            *FIGHTER_STATUS_KIND_ATTACK_DASH,
            *FIGHTER_STATUS_KIND_ATTACK_S3,
            *FIGHTER_STATUS_KIND_ATTACK_HI3,
            *FIGHTER_STATUS_KIND_ATTACK_LW3,
            *FIGHTER_STATUS_KIND_ATTACK_S4_START,
            *FIGHTER_STATUS_KIND_ATTACK_S4_HOLD,
            *FIGHTER_STATUS_KIND_ATTACK_S4,
            *FIGHTER_STATUS_KIND_ATTACK_HI4_START,
            *FIGHTER_STATUS_KIND_ATTACK_HI4_HOLD,
            *FIGHTER_STATUS_KIND_ATTACK_HI4,
            *FIGHTER_STATUS_KIND_ATTACK_LW4_START,
            *FIGHTER_STATUS_KIND_ATTACK_LW4_HOLD,
            *FIGHTER_STATUS_KIND_ATTACK_LW4
        ].contains(&fighter.global_table[STATUS_KIND].get_i32()) {
            WorkModule::set_int(fighter.module_accessor, 0, FIGHTER_INSTANCE_WORK_ID_INT_USED_GROUND_NORMALS);
        }
    }

    /// Disables an aerial. Used for characters who can cancel aerials into other aerials.
    pub unsafe fn disable_aerial(fighter: &mut L2CFighterCommon, aerial_mask: i32) {
        if !CancelModule::is_enable_cancel(fighter.module_accessor) {
            let mut used_aerials = WorkModule::get_int(fighter.module_accessor, FIGHTER_INSTANCE_WORK_ID_INT_USED_AERIALS);
            if used_aerials & aerial_mask == 0 {
                used_aerials += aerial_mask;
            }
            WorkModule::set_int(fighter.module_accessor, used_aerials, FIGHTER_INSTANCE_WORK_ID_INT_USED_AERIALS);
        }
    }

    /// Checks if certain aerials are enabled.
    pub unsafe fn check_enabled_aerial(fighter: &mut L2CFighterCommon) -> bool {
        if WorkModule::is_flag(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_FLAG_NORMAL_CANCEL)
        && !CancelModule::is_enable_cancel(fighter.module_accessor) {
            let enabled_mask = WorkModule::get_int(fighter.module_accessor, FIGHTER_STATUS_WORK_ID_INT_ENABLED_AERIALS);
            let used_mask = WorkModule::get_int(fighter.module_accessor, FIGHTER_INSTANCE_WORK_ID_INT_USED_AERIALS);
            let attack_air_kind = ControlModule::get_attack_air_kind(fighter.module_accessor);
            let aerial_flag;
            match attack_air_kind {
                1 => aerial_flag = ATTACK_AIR_N_MASK,
                2 => aerial_flag = ATTACK_AIR_F_MASK,
                3 => aerial_flag = ATTACK_AIR_B_MASK,
                4 => aerial_flag = ATTACK_AIR_HI_MASK,
                5 => aerial_flag = ATTACK_AIR_LW_MASK,
                _ => aerial_flag = 0b00000
            }
            return enabled_mask & aerial_flag != 0 && used_mask & aerial_flag == 0;
        }
        true
    }

    /// Resets your aerial cancel string, enabling all aerials again.
    pub unsafe fn reset_used_aerials(fighter: &mut L2CFighterCommon) {
        WorkModule::set_int(fighter.module_accessor, 0, FIGHTER_INSTANCE_WORK_ID_INT_USED_AERIALS);
    }

    /// Handles adding or subtracting meter.
    pub unsafe fn update_meter(object: *mut BattleObject, amount: f32, meter_max: f32, meter_const: i32) {
        let module_accessor = (*object).module_accessor;
        let mut meter = WorkModule::get_float(module_accessor, meter_const);
        meter += amount;
        if meter < 0.0 {
            meter == 0.0;
        }
        if meter > meter_max {
            meter = meter_max;
        }
        WorkModule::set_float(module_accessor, meter, meter_const);
    }
}

#[allow(non_snake_case)]
pub mod MiscModule {
    use super::*;

    /// Checks if your current status is one where you're being damaged. If is_prev is true, it checks your previous status instead.
    pub unsafe fn is_damage_check(module_accessor: *mut BattleObjectModuleAccessor, is_prev: bool) -> bool {
        let status : i32;
        if is_prev {
            status = StatusModule::prev_status_kind(module_accessor, 0);
        }
        else {
            status = StatusModule::status_kind(module_accessor);
        }
        if FighterStopModuleImpl::is_damage_stop(module_accessor)
        || WorkModule::is_flag(module_accessor, *FIGHTER_INSTANCE_WORK_ID_FLAG_CAPTURE_YOSHI)
        || WorkModule::is_flag(module_accessor, *FIGHTER_INSTANCE_WORK_ID_FLAG_GANON_SPECIAL_S_DAMAGE_FALL_GROUND)
        || WorkModule::is_flag(module_accessor, *FIGHTER_INSTANCE_WORK_ID_FLAG_GANON_SPECIAL_S_DAMAGE_FALL_AIR)
        || (*FIGHTER_STATUS_KIND_CAPTURE_PULLED..=*FIGHTER_STATUS_KIND_DAMAGE_FALL).contains(&status)
        || (*FIGHTER_STATUS_KIND_DOWN..=*FIGHTER_STATUS_KIND_LAY_DOWN).contains(&status)
        || (*FIGHTER_STATUS_KIND_DOWN_DAMAGE..=*FIGHTER_STATUS_KIND_DOWN_REFLECT_LR).contains(&status)
        || (*FIGHTER_STATUS_KIND_FURAFURA_STAND..=*FIGHTER_STATUS_KIND_BIND).contains(&status)
        || (*FIGHTER_STATUS_KIND_SLIP..=*FIGHTER_STATUS_KIND_SLIP_WAIT).contains(&status)
        || (*FIGHTER_STATUS_KIND_TREAD_DAMAGE..=*FIGHTER_STATUS_KIND_ICE_JUMP).contains(&status)
        || (*FIGHTER_STATUS_KIND_LINK_FINAL..=*FIGHTER_STATUS_KIND_PIT_FALL).contains(&status)
        || (*FIGHTER_STATUS_KIND_SWALLOWED..=*FIGHTER_STATUS_KIND_CAPTURE_DAMAGE_YOSHI).contains(&status)
        || (*FIGHTER_STATUS_KIND_CATCHED_REFLET..=*FIGHTER_STATUS_KIND_CAPTURE_MASTERHAND).contains(&status)
        || status == *FIGHTER_STATUS_KIND_GIMMICK_EATEN
        || (*FIGHTER_STATUS_KIND_CAPTURE_ITEM..=*FIGHTER_STATUS_KIND_CAPTURE_CLAPTRAP).contains(&status)
        || (*FIGHTER_STATUS_KIND_FINAL_VISUAL_ATTACK_OTHER..=*FIGHTER_STATUS_KIND_RIDLEY_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_CATCHED_RIDLEY..=*FIGHTER_STATUS_KIND_STABBED_DAMAGE).contains(&status)
        || (*FIGHTER_STATUS_KIND_SWING_GAOGAEN_CATCHED..=*FIGHTER_STATUS_KIND_SWING_GAOGAEN_FAILURE).contains(&status)
        || (*FIGHTER_STATUS_KIND_SHEIK_FINAL_CAPTURE..=*FIGHTER_STATUS_KIND_CAPTURE_WAIT_OCTOPUS).contains(&status)
        || (*FIGHTER_STATUS_KIND_SIMON_FINAL_TARGET_START..=*FIGHTER_STATUS_KIND_YOSHI_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_SUICIDE_BOMB..=*FIGHTER_STATUS_KIND_TANTAN_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_DAMAGE_FLY_REFLECT_JUMP_BOARD..=*FIGHTER_STATUS_KIND_EDGE_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_CAPTURE_TRAIL_KEYHOLE..=*FIGHTER_STATUS_KIND_TRAIL_FINAL_TARGET_END).contains(&status) {
            return true;
        }
        false
    }

    /// Checks if your current status is considered getting damaged, but is different from just normally getting hit.
    /// If is_prev is true, it checks your previous status instead.
    pub unsafe fn is_illegal_status(module_accessor: *mut BattleObjectModuleAccessor, is_prev: bool) -> bool {
        let status : i32;
        if is_prev {
            status = StatusModule::prev_status_kind(module_accessor, 0);
        }
        else {
            status = StatusModule::status_kind(module_accessor);
        }
        if WorkModule::is_flag(module_accessor, *FIGHTER_INSTANCE_WORK_ID_FLAG_CAPTURE_YOSHI)
        || (*FIGHTER_STATUS_KIND_CAPTURE_PULLED..=*FIGHTER_STATUS_KIND_THROWN).contains(&status)
        || (*FIGHTER_STATUS_KIND_FURAFURA_STAND..=*FIGHTER_STATUS_KIND_BIND).contains(&status)
        || (*FIGHTER_STATUS_KIND_SLIP..=*FIGHTER_STATUS_KIND_SLIP_WAIT).contains(&status)
        || (*FIGHTER_STATUS_KIND_CLUNG_GANON..=*FIGHTER_STATUS_KIND_ICE_JUMP).contains(&status)
        || (*FIGHTER_STATUS_KIND_LINK_FINAL..=*FIGHTER_STATUS_KIND_PIT_FALL).contains(&status)
        || (*FIGHTER_STATUS_KIND_SWALLOWED..=*FIGHTER_STATUS_KIND_CAPTURE_DAMAGE_YOSHI).contains(&status)
        || (*FIGHTER_STATUS_KIND_CATCHED_REFLET..=*FIGHTER_STATUS_KIND_CAPTURE_MASTERHAND).contains(&status)
        || status == *FIGHTER_STATUS_KIND_GIMMICK_EATEN
        || (*FIGHTER_STATUS_KIND_CAPTURE_ITEM..=*FIGHTER_STATUS_KIND_CAPTURE_CLAPTRAP).contains(&status)
        || (*FIGHTER_STATUS_KIND_FINAL_VISUAL_ATTACK_OTHER..=*FIGHTER_STATUS_KIND_RIDLEY_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_CATCHED_RIDLEY..=*FIGHTER_STATUS_KIND_STABBED_DAMAGE).contains(&status)
        || (*FIGHTER_STATUS_KIND_SWING_GAOGAEN_CATCHED..=*FIGHTER_STATUS_KIND_SWING_GAOGAEN_FAILURE).contains(&status)
        || (*FIGHTER_STATUS_KIND_SHEIK_FINAL_CAPTURE..=*FIGHTER_STATUS_KIND_CAPTURE_WAIT_OCTOPUS).contains(&status)
        || (*FIGHTER_STATUS_KIND_SIMON_FINAL_TARGET_START..=*FIGHTER_STATUS_KIND_YOSHI_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_SUICIDE_BOMB..=*FIGHTER_STATUS_KIND_TANTAN_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_DAMAGE_FLY_REFLECT_JUMP_BOARD..=*FIGHTER_STATUS_KIND_EDGE_FINAL_TARGET_END).contains(&status)
        || (*FIGHTER_STATUS_KIND_CAPTURE_TRAIL_KEYHOLE..=*FIGHTER_STATUS_KIND_TRAIL_FINAL_TARGET_END).contains(&status) {
            return true;
        }
        false
    }

    /// Forces a wall jump. Will be moved to FGCModule eventually.
    pub unsafe fn wall_jump_check(fighter: &mut L2CFighterCommon) {
        let right_side = GroundModule::is_wall_touch_line(fighter.module_accessor, *GROUND_TOUCH_FLAG_RIGHT_SIDE as u32);
        let left_side = GroundModule::is_wall_touch_line(fighter.module_accessor, *GROUND_TOUCH_FLAG_LEFT_SIDE as u32);
        if right_side {
            if fighter.global_table[CMD_CAT1] & *FIGHTER_PAD_CMD_CAT1_FLAG_WALL_JUMP_LEFT != 0 {
                StatusModule::change_status_request_from_script(fighter.module_accessor, *FIGHTER_STATUS_KIND_WALL_JUMP, true);
            }
        }
        else if left_side {
            if fighter.global_table[CMD_CAT1] & *FIGHTER_PAD_CMD_CAT1_FLAG_WALL_JUMP_RIGHT != 0 {
                StatusModule::change_status_request_from_script(fighter.module_accessor, *FIGHTER_STATUS_KIND_WALL_JUMP, true);
            }
        }
    }

    /// Creates the "critical hit" effect. Will be replaced later with a better implementation.
    pub unsafe fn critical_zoom(fighter: &mut L2CFighterCommon, rate: u8, frames: f32, zoom: f32) {
        if !SoundModule::is_playing(fighter.module_accessor, Hash40::new("se_common_finishhit")) {
            macros::EFFECT(fighter, Hash40::new("sys_bg_criticalhit"), Hash40::new("top"), 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, false);
            if rate != 0 {
                SlowModule::set_whole(fighter.module_accessor, rate, 0);
            }
            if FighterUtil::get_opponent_fighter_num(fighter.module_accessor, true) < 2 {
                macros:: CAM_ZOOM_IN_arg5(fighter, frames, 0.0, zoom, 0.0, 0.0);
            }
            macros::PLAY_SE(fighter, Hash40::new("se_common_criticalhit"));
        }
    }

    /// Used in Fighting Game Mode to reduce a fighter's HP to their Fighting Game Mode value.
    pub unsafe fn set_hp(fighter: &mut L2CFighterCommon, hp: f32) {
        if DamageModule::damage(fighter.module_accessor, 0) < hp
        && !smashball::is_training_mode() {
            let dmg = hp - DamageModule::damage(fighter.module_accessor, 0);
            DamageModule::add_damage(fighter.module_accessor, dmg, 0);
        }
    }

    /// Sets all of the values needed for Taunt Hold/Loops.
    pub unsafe fn set_appeal_loop(module_accessor: *mut BattleObjectModuleAccessor, is_loop: bool, loop_mot: u64, restart_frame: i32, buttons: i32) {
        WorkModule::set_int(module_accessor, buttons, FIGHTER_STATUS_APPEAL_WORK_INT_APPEAL_HELD_BUTTON);
        WorkModule::set_int(module_accessor, restart_frame, FIGHTER_STATUS_APPEAL_WORK_INT_APPEAL_RESTART_FRAME);
        WorkModule::set_int64(
            module_accessor,
            loop_mot as i64,
            FIGHTER_STATUS_APPEAL_WORK_INT_APPEAL_LOOP_MOT
        );
        WorkModule::set_int64(
            module_accessor,
            smash::hash40("invalid") as i64,
            FIGHTER_STATUS_APPEAL_WORK_INT_APPEAL_ACTION_MOT
        );
        if is_loop {
            WorkModule::on_flag(module_accessor, FIGHTER_STATUS_APPEAL_WORK_FLAG_APPEAL_LOOP);
        }
        else {
            WorkModule::on_flag(module_accessor, FIGHTER_STATUS_APPEAL_WORK_FLAG_APPEAL_HOLD);
        }
    }
}
