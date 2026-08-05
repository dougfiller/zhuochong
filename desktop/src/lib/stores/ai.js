// AI 连接状态全局 Store
// 用于跨组件持久化 AI 连接测试状态，避免每次切换页面都重新测试

import { writable } from 'svelte/store';

// 创建 AI 状态 store
function createAiStore() {
    const { subscribe, set, update } = writable({
        // 测试状态: null(未测试), 'testing', 'success', 'error'
        textTestStatus: null,
        textTestMessage: '',
        // 连接是否验证成功
        textConnectionVerified: false,
        // 上次测试的配置指纹（用于判断配置是否变化）
        lastTestedConfigHash: null,
    });

    return {
        subscribe,

        // 开始测试
        startTesting: () => update(s => ({
            ...s,
            textTestStatus: 'testing',
            textTestMessage: ''
        })),

        // 测试成功
        setSuccess: (message) => update(s => ({
            ...s,
            textTestStatus: 'success',
            textTestMessage: message,
            textConnectionVerified: true
        })),

        // 测试失败
        setError: (message) => update(s => ({
            ...s,
            textTestStatus: 'error',
            textTestMessage: message,
            textConnectionVerified: false
        })),

        // 重置状态（提供商变更时）
        reset: () => update(s => ({
            ...s,
            textTestStatus: null,
            textTestMessage: '',
            textConnectionVerified: false,
            lastTestedConfigHash: null
        })),

        // 设置配置指纹
        setConfigHash: (hash) => update(s => ({
            ...s,
            lastTestedConfigHash: hash
        })),

        // 计算配置指纹
        getConfigHash: (config) => {
            if (!config?.text_model) return null;
            const { provider, endpoint, model, api_key } = config.text_model;
            return `${provider}|${endpoint}|${model}|${api_key || ''}`;
        }
    };
}

export const aiStore = createAiStore();
