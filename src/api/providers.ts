// Compatibility entry point: all supplier settings use the local configuration API.
export { getProviders, createProvider, updateProvider, deleteProvider, getProviderModels, fetchProviderModels, testProviderWebSearch } from '../api';
export type { Provider, ProviderModel, WebSearchTestResult } from '../api';
export { getProviders as listProviders } from '../api';
