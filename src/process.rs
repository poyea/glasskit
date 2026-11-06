// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Settings of the whole process.

use std::ffi::c_void;

use windows::Win32::System::Threading::{
    GetCurrentProcess, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetPriorityClass,
    SetProcessInformation, SetProcessWorkingSetSize,
};

/// Turn efficiency mode on or off. It is the same as "Efficiency mode" in Task Manager: the
/// process runs with EcoQoS and at idle priority. Turn it on while the app only waits, and off
/// while a window shows. When it turns on, the process also releases its unused memory pages.
pub fn set_efficiency_mode(on: bool) {
    unsafe {
        let process = GetCurrentProcess();
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: if on {
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED
            } else {
                0
            },
        };
        let _ = SetProcessInformation(
            process,
            ProcessPowerThrottling,
            &state as *const _ as *const c_void,
            size_of_val(&state) as u32,
        );
        let class = if on {
            IDLE_PRIORITY_CLASS
        } else {
            NORMAL_PRIORITY_CLASS
        };
        let _ = SetPriorityClass(process, class);
        if on {
            let _ = SetProcessWorkingSetSize(process, usize::MAX, usize::MAX);
        }
    }
}
