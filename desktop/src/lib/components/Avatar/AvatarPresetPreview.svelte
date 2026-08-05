<script>
  import AvatarCanvas from './AvatarCanvas.svelte';
  import { getAvatarPresetOption, normalizeAvatarPresetId } from './avatarPresetRegistry.js';

  export let presetId = 'original-standard';
  export let selected = false;

  $: normalizedPresetId = normalizeAvatarPresetId(presetId);
  $: presetOption = getAvatarPresetOption(normalizedPresetId);
  $: previewState = {
    mode: 'working',
    appName: 'Work Review',
    contextLabel: '办公中',
    hint: '',
    isIdle: false,
    isGeneratingReport: false,
    avatarOpacity: selected ? 0.96 : 0.9,
    avatarPreset: normalizedPresetId,
  };
  $: previewInputActivity = presetOption.previewInputActivity;
  $: previewMotionBeat = presetOption.previewMotionBeat ?? 18;
</script>

<div class="pointer-events-none h-full w-full overflow-hidden rounded-[20px]">
  <div class="h-full w-full scale-[1.08] origin-center">
    <AvatarCanvas
      state={previewState}
      inputActivity={previewInputActivity}
      transitionClass=""
      motionBeat={previewMotionBeat}
    />
  </div>
</div>
