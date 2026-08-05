import { writable } from 'svelte/store';

/**
 * 录制状态 store。
 *
 * App.svelte 在 onMount 时通过 get_recording_state 初始化，
 * 并监听 recording-state-changed 事件实时更新。
 * Overview 和 Timeline 读取此 store 来控制"实时"指示圆点的颜色/动画
 * （停止记录后圆点应变灰、停止脉冲），而不是仅看是否"今天"模式。
 *
 * 对应后端事件 payload：{ isRecording: bool, isPaused: bool }
 */
const DEFAULT_STATE = {
  isRecording: true,
  isPaused: false,
};

function createRecordingStore() {
  const { subscribe, set } = writable(DEFAULT_STATE);

  return {
    subscribe,
    set,
    setState: (isRecording, isPaused) =>
      set({ isRecording: Boolean(isRecording), isPaused: Boolean(isPaused) }),
    reset: () => set(DEFAULT_STATE),
  };
}

export const recordingStore = createRecordingStore();

/** 是否正在活跃录制（录制中且未暂停）—— 组件直接用的派生布尔。 */
export function isActiveRecording(state) {
  return Boolean(state?.isRecording) && !state?.isPaused;
}
