<script>
  import { createEventDispatcher } from 'svelte';
  import SettingsAppearance from './SettingsAppearance.svelte';

  export let config;

  const dispatch = createEventDispatcher();

  function handleChange(event) {
    dispatch('change', event.detail);
  }
</script>

<SettingsAppearance
  bind:config
  mode="avatar-only"
  on:change={handleChange}
/>
