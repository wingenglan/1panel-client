<template>
    <div :key="$route.fullPath" id="dashboard">
        <RouterButton
            show-expires-at
            :buttons="[
                {
                    label: i18n.global.t('menu.home'),
                    path: '/',
                },
            ]"
        />

        <el-alert v-if="!isSafety && showEntranceWarn" class="card-interval" type="warning" @close="hideEntrance">
            <template #title>
                <span class="flx-align-center">
                    <span>{{ $t('home.entranceHelper') }}</span>
                    <el-link
                        style="font-size: 12px; margin-left: 5px"
                        icon="Position"
                        v-if="isAdmin"
                        @click="jumpToPath(router, '/settings/safe')"
                        type="primary"
                    >
                        {{ $t('firewall.quickJump') }}
                    </el-link>
                </span>
            </template>
        </el-alert>

        <el-row :gutter="7" class="card-interval">
            <el-col :xs="24" :sm="24" :md="16" :lg="16" :xl="16">
                <CardWithHeader :header="$t('menu.home')" height="166px">
                    <template #header-r>
                        <el-button
                            class="h-button-setting"
                            :disabled="!isAdminOrNodeAdmin"
                            @click="quickJumpRef.acceptParams()"
                            link
                            icon="Setting"
                        />
                    </template>
                    <template #body>
                        <div class="h-overview">
                            <el-row>
                                <el-col :span="6" v-for="item in baseInfo.quickJump" :key="item.name">
                                    <span>{{ $t(item.title, 2) }}</span>
                                    <div class="count">
                                        <el-tooltip
                                            v-if="item.alias || item.detail.length > 20"
                                            :content="item.detail"
                                            placement="bottom"
                                        >
                                            <el-button
                                                link
                                                :disabled="!checkPermission('File')"
                                                type="primary"
                                                @click="quickJump(item)"
                                            >
                                                {{ item.alias || item.detail.substring(0, 18) + '...' }}
                                            </el-button>
                                        </el-tooltip>
                                        <el-button
                                            link
                                            :disabled="!checkPermission(item.name)"
                                            type="primary"
                                            @click="quickJump(item)"
                                            v-else
                                        >
                                            {{ item.detail }}
                                        </el-button>
                                    </div>
                                </el-col>
                            </el-row>
                        </div>
                    </template>
                </CardWithHeader>
                <CardWithHeader :header="$t('commons.table.status')" class="card-interval">
                    <template #body>
                        <SystemStatus ref="statusRef" style="margin-bottom: 33px" />
                    </template>
                </CardWithHeader>
                <CardWithHeader
                    :header="$t('menu.monitor')"
                    class="card-interval chart-card"
                    @mouseenter="refreshOptionsOnHover"
                >
                    <template #header-r>
                        <el-radio-group
                            style="float: right; margin-left: 5px"
                            v-model="chartOption"
                            @change="changeOption"
                        >
                            <el-radio-button value="network">{{ $t('home.network') }}</el-radio-button>
                            <el-radio-button value="io">{{ $t('home.io') }}</el-radio-button>
                        </el-radio-group>
                        <el-select
                            v-if="chartOption === 'network'"
                            @change="onLoadBaseInfo(false, 'network')"
                            v-model="searchInfo.netOption"
                            class="p-w-200 float-right"
                        >
                            <template #prefix>{{ $t('home.networkCard') }}</template>
                            <el-option
                                v-for="item in netOptions"
                                :key="item"
                                :label="item == 'all' ? $t('commons.table.all') : item"
                                :value="item"
                            />
                        </el-select>
                        <el-select
                            v-if="chartOption === 'io'"
                            v-model="searchInfo.ioOption"
                            @change="onLoadBaseInfo(false, 'io')"
                            class="p-w-200 float-right"
                        >
                            <template #prefix>{{ $t('home.disk') }}</template>
                            <el-option
                                v-for="item in ioOptions"
                                :key="item"
                                :label="item == 'all' ? $t('commons.table.all') : item"
                                :value="item"
                            />
                        </el-select>
                    </template>
                    <template #body>
                        <div style="position: relative; margin-top: 60px">
                            <div class="monitor-tags" :style="monitorTagsStyle" v-if="chartOption === 'network'">
                                <el-tag>
                                    {{ $t('monitor.up') }}: {{ computeSizeFromKBs(currentChartInfo.netBytesSent) }}
                                </el-tag>
                                <el-tag>
                                    {{ $t('monitor.down') }}: {{ computeSizeFromKBs(currentChartInfo.netBytesRecv) }}
                                </el-tag>
                                <el-tag>{{ $t('home.totalSend') }}: {{ computeSize(currentInfo.netBytesSent) }}</el-tag>
                                <el-tag>{{ $t('home.totalRecv') }}: {{ computeSize(currentInfo.netBytesRecv) }}</el-tag>
                            </div>
                            <div class="monitor-tags" :style="monitorTagsStyle" v-if="chartOption === 'io'">
                                <el-tag>{{ $t('monitor.read') }}: {{ currentChartInfo.ioReadBytes }} MB</el-tag>
                                <el-tag>{{ $t('monitor.write') }}: {{ currentChartInfo.ioWriteBytes }} MB</el-tag>
                                <el-tag>
                                    {{ $t('home.rwPerSecond') }}: {{ currentChartInfo.ioCount }}
                                    {{ $t('commons.units.time') }}/s
                                </el-tag>
                                <el-tag>{{ $t('home.ioDelay') }}: {{ currentChartInfo.ioTime }} ms</el-tag>
                            </div>

                            <div v-if="chartOption === 'io'" style="margin-top: 40px" class="mobile-monitor-chart">
                                <v-charts
                                    height="383px"
                                    id="ioChart"
                                    type="line"
                                    :option="chartsOption['ioChart']"
                                    :dataZoom="true"
                                />
                            </div>
                            <div v-if="chartOption === 'network'" style="margin-top: 40px" class="mobile-monitor-chart">
                                <v-charts
                                    height="383px"
                                    id="networkChart"
                                    type="line"
                                    :option="chartsOption['networkChart']"
                                    :dataZoom="true"
                                />
                            </div>
                        </div>
                    </template>
                </CardWithHeader>
            </el-col>
            <el-col :xs="24" :sm="24" :md="8" :lg="8" :xl="8" class="dashboard-right">
                <el-carousel
                    class="my-carousel"
                    :class="{ 'no-indicator': carouselItemCount <= 1 }"
                    :key="simpleNodes.length + carouselItemCount"
                    height="368px"
                    indicator-position=""
                    arrow="never"
                    :autoplay="!showMemoCarousel || !memoEditing"
                >
                    <el-carousel-item key="systemInfo">
                        <CardWithHeader :header="$t('home.systemInfo')">
                            <template #header-r>
                                <el-popover
                                    popper-class="dashboard-carousel-popover"
                                    placement="bottom"
                                    :title="$t('home.carouselSetting')"
                                    width="220"
                                    trigger="click"
                                >
                                    <div class="dashboard-carousel-setting">
                                        <div class="setting-item mt-2">
                                            <span>{{ $t('home.systemInfo') }}</span>
                                            <div class="mr-4">-</div>
                                        </div>
                                        <div class="setting-item mt-2">
                                            <span>{{ $t('home.memo') }}</span>
                                            <el-switch
                                                v-model="memoCarouselSetting"
                                                active-value="Enable"
                                                inactive-value="Disable"
                                                @change="
                                                    (val) => updateDashboardCarouselSetting('DashboardMemoVisible', val)
                                                "
                                            />
                                        </div>
                                        <div class="setting-item">
                                            <span>{{ $t('setting.panel') }}</span>
                                            <el-switch
                                                v-model="simpleNodeCarouselSetting"
                                                active-value="Enable"
                                                inactive-value="Disable"
                                                @change="
                                                    (val) =>
                                                        updateDashboardCarouselSetting(
                                                            'DashboardSimpleNodeVisible',
                                                            val,
                                                        )
                                                "
                                            />
                                        </div>
                                    </div>
                                    <template #reference>
                                        <el-button
                                            class="h-button-setting"
                                            :disabled="!isAdminOrNodeAdmin"
                                            link
                                            icon="Setting"
                                        />
                                    </template>
                                </el-popover>
                                <el-tooltip :content="$t('commons.button.refresh')" placement="top">
                                    <el-button class="h-button-setting" @click="refreshDashboard" link icon="Refresh" />
                                </el-tooltip>
                                <el-tooltip :content="$t('home.tooltipSensitiveInfo')" placement="top">
                                    <el-button
                                        class="h-button-setting"
                                        @click="toggleSensitiveInfo"
                                        link
                                        :icon="showSensitiveInfo ? 'View' : 'Hide'"
                                    />
                                </el-tooltip>
                                <el-tooltip :content="$t('commons.button.copy')" placement="top">
                                    <el-button class="h-button-setting" @click="handleCopy" link icon="CopyDocument" />
                                </el-tooltip>
                            </template>
                            <template #body>
                                <el-scrollbar>
                                    <el-descriptions :column="1" class="ml-5 -mt-2 h-systemInfo" border>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.hostname') }}</span>
                                            </template>
                                            {{ showSensitiveInfo ? baseInfo.hostname : '****' }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.platformVersion') }}</span>
                                            </template>
                                            {{
                                                baseInfo.prettyDistro
                                                    ? baseInfo.prettyDistro
                                                    : baseInfo.platformVersion
                                                      ? baseInfo.platform + '-' + baseInfo.platformVersion
                                                      : baseInfo.platform
                                            }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.kernelVersion') }}</span>
                                            </template>
                                            {{ baseInfo.kernelVersion }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.kernelArch') }}</span>
                                            </template>
                                            {{ baseInfo.kernelArch }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.ip') }}</span>
                                            </template>
                                            {{ showSensitiveInfo ? baseInfo.ipV4Addr : '****' }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            v-if="baseInfo.httpProxy && baseInfo.httpProxy !== 'noProxy'"
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.proxy') }}</span>
                                                {{ baseInfo.httpProxy }}
                                            </template>
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.uptime') }}</span>
                                            </template>
                                            {{ currentInfo.timeSinceUptime }}
                                        </el-descriptions-item>
                                        <el-descriptions-item
                                            class-name="system-content"
                                            label-class-name="system-label"
                                        >
                                            <template #label>
                                                <span class="system-label">{{ $t('home.runningTime') }}</span>
                                            </template>
                                            {{ formatUptime(currentInfo.runningTime) }}
                                        </el-descriptions-item>
                                    </el-descriptions>
                                </el-scrollbar>
                            </template>
                        </CardWithHeader>
                    </el-carousel-item>
                    <el-carousel-item key="memoInfo" v-if="showMemoCarousel">
                        <CardWithHeader :header="$t('home.memo')" class="memo-card">
                            <template #header-r>
                                <el-tooltip v-if="!memoEditing" :content="$t('commons.button.edit')" placement="top">
                                    <el-button
                                        class="h-button-setting"
                                        :disabled="!isAdminOrNodeAdmin"
                                        @click="startMemoEdit"
                                        link
                                        icon="Edit"
                                    />
                                </el-tooltip>
                                <el-tooltip v-if="memoEditing" :content="$t('commons.button.save')" placement="top">
                                    <el-button
                                        class="h-button-setting"
                                        @click="saveMemo"
                                        link
                                        icon="Check"
                                        :loading="memoSaving"
                                    />
                                </el-tooltip>
                                <el-tooltip v-if="memoEditing" :content="$t('commons.button.cancel')" placement="top">
                                    <el-button class="h-button-setting" @click="cancelMemoEdit" link icon="Close" />
                                </el-tooltip>
                            </template>
                            <template #body>
                                <el-scrollbar height="286px">
                                    <div class="memo-container ml-5 mr-5">
                                        <el-input
                                            v-if="memoEditing"
                                            v-model="memoEditContent"
                                            type="textarea"
                                            :rows="10"
                                            :maxlength="500"
                                            :placeholder="$t('home.memoPlaceholder')"
                                            show-word-limit
                                        />
                                        <div v-else class="memo-content">
                                            <MarkDownEditor v-if="memoContent" :content="memoContent" />
                                            <div v-else class="memo-empty">
                                                <span class="memo-placeholder">
                                                    {{ $t('home.memoPlaceholder') }}
                                                </span>
                                            </div>
                                        </div>
                                    </div>
                                </el-scrollbar>
                            </template>
                        </CardWithHeader>
                    </el-carousel-item>
                    <el-carousel-item key="simpleNode" v-if="showSimpleNode()">
                        <CardWithHeader :header="$t('setting.panel')">
                            <template #header-r>
                                <el-tooltip :content="$t('xpack.node.panelItem')" placement="top">
                                    <el-button
                                        class="h-button-setting"
                                        @click="routerToNameWithQuery('SimpleNode', { uncached: 'true' })"
                                        link
                                        icon="Setting"
                                    />
                                </el-tooltip>
                            </template>
                            <template #body>
                                <el-scrollbar height="286px">
                                    <div class="simple-node cursor-pointer" v-for="row in simpleNodes" :key="row.id">
                                        <el-row :gutter="5">
                                            <el-col :span="21">
                                                <div class="name">
                                                    {{ row.name }}
                                                    <Status :status="row.status" :msg="row.message" />
                                                </div>
                                                <div class="detail">
                                                    {{ loadSource(row) }}
                                                </div>
                                            </el-col>

                                            <el-col :span="1">
                                                <el-button
                                                    @click="jumpPanel(row)"
                                                    size="small"
                                                    :disabled="row.status !== 'Healthy'"
                                                    class="visit"
                                                    round
                                                    plain
                                                    type="primary"
                                                >
                                                    {{ $t('commons.button.visit') }}
                                                </el-button>
                                            </el-col>
                                        </el-row>
                                        <div class="h-app-divider" />
                                    </div>
                                </el-scrollbar>
                            </template>
                        </CardWithHeader>
                    </el-carousel-item>
                </el-carousel>

                <AppLauncher ref="appRef" class="card-interval dashboard-app" />
            </el-col>
        </el-row>

        <QuickJump @search="onLoadBaseInfo(false, 'all')" ref="quickJumpRef" />

        <DialogPro v-model="welcomeOpen" size="w-70" id="welcomeDialog">
            <div ref="shadowContainer" />
        </DialogPro>
    </div>
</template>

<script lang="ts" setup>
import { onMounted, onBeforeUnmount, ref, reactive, computed, nextTick } from 'vue';
import SystemStatus from '@/views/home/status/index.vue';
import AppLauncher from '@/views/home/app/index.vue';
import VCharts from '@/components/v-charts/index.vue';
import QuickJump from '@/views/home/quick/index.vue';
import CardWithHeader from '@/components/card-with-header/index.vue';
import MarkDownEditor from '@/components/mkdown-editor/index.vue';
import i18n from '@/lang';
import { Dashboard } from '@/api/interface/dashboard';
import { dateFormatForSecond, formatUptime } from '@/utils/date';
import { computeSize, computeSizeFromKBs } from '@/utils/size';
import { jumpToPath } from '@/utils/router';
import { copyText } from '@/utils/clipboard';
import { useRouter } from 'vue-router';
import { loadBaseInfo, loadCurrentInfo } from '@/api/modules/dashboard';
import { getIOOptions, getNetworkOptions } from '@/api/modules/host';
import {
    getSettingBaseInfo,
    getAgentSettingInfo,
    listAllSimpleNodes,
    loadUpgradeInfo,
    getMemo,
    updateMemo,
    updateSetting,
} from '@/api/modules/setting';
import { routerToFileWithPath, routerToNameWithQuery, routerToPath } from '@/utils/router';
import { getWelcomePage } from '@/api/modules/auth';
import {
    clearDashboardCache,
    clearDashboardCacheByPrefix,
    getDashboardCache,
    setDashboardCache,
} from '@/utils/dashboardCache';
import { MsgSuccess } from '@/utils/message';
import { useCan } from '@/composables/useMenuManagePermission';
const router = useRouter();
import { useGlobalStore } from '@/composables/useGlobalStore';
const {
    showEntranceWarn,
    defaultNetwork,
    defaultIO,
    isAdmin,
    isOnRestart,
    hasNewVersion,
    isAdminOrNodeAdmin,
    isXpackOrEE,
} = useGlobalStore();

const DASHBOARD_CACHE_TTL = {
    safeStatus: 10 * 60 * 1000,
    netOptions: 60 * 60 * 1000,
    ioOptions: 60 * 60 * 1000,
};
const monitorChartGrid = { left: 65, right: 65, bottom: '20%' };
const monitorChartLegend = { top: 0, bottom: 'auto' };
const monitorTagsStyle = {
    left: `${monitorChartGrid.left}px`,
    right: `${monitorChartGrid.right}px`,
};
const monitorChartEmptyLength = 20;
const loadMonitorEmptyData = () => Array.from({ length: monitorChartEmptyLength }, () => null);
const loadMonitorEmptyTime = () => Array.from({ length: monitorChartEmptyLength }, () => '');
const loadMonitorChartData = (data: Array<number>) => (data.length === 0 ? loadMonitorEmptyData() : data);
const loadMonitorChartTime = (data: Array<string>) => (data.length === 0 ? loadMonitorEmptyTime() : data);
const loadIOChartOption = () => ({
    xData: loadMonitorChartTime(timeIODatas.value),
    yData: [
        {
            name: i18n.global.t('monitor.read'),
            data: loadMonitorChartData(ioReadBytes.value),
        },
        {
            name: i18n.global.t('monitor.write'),
            data: loadMonitorChartData(ioWriteBytes.value),
        },
    ],
    grid: monitorChartGrid,
    legend: monitorChartLegend,
    formatStr: 'MB',
});
const loadNetworkChartOption = () => ({
    xData: loadMonitorChartTime(timeNetDatas.value),
    yData: [
        {
            name: i18n.global.t('monitor.up'),
            data: loadMonitorChartData(netBytesSents.value),
        },
        {
            name: i18n.global.t('monitor.down'),
            data: loadMonitorChartData(netBytesRecvs.value),
        },
    ],
    grid: monitorChartGrid,
    legend: monitorChartLegend,
    formatStr: 'KB/s',
});

const statusRef = ref();
const appRef = ref();

const isSafety = ref();

const welcomeOpen = ref();
const shadowContainer = ref();

const chartOption = ref('network');
let timer: NodeJS.Timer | null = null;
let isInit = ref<boolean>(true);
let isStatusInit = ref<boolean>(true);
let isActive = ref(true);
let isCurrentActive = ref(true);

const showSensitiveInfo = ref(true);

const ioReadBytes = ref<Array<number>>([]);
const ioWriteBytes = ref<Array<number>>([]);
const netBytesSents = ref<Array<number>>([]);
const netBytesRecvs = ref<Array<number>>([]);
const timeIODatas = ref<Array<string>>([]);
const timeNetDatas = ref<Array<string>>([]);

const simpleNodes = ref([]);
const ioOptions = ref();
const netOptions = ref();
const netOptionsFromCache = ref(false);
const ioOptionsFromCache = ref(false);
const hasRefreshedOptionsOnHover = ref(false);

const quickJumpRef = ref();
const quickJumpPermissionMap = Object.fromEntries(
    [
        ['Agent', 'ai_agent_view'],
        ['Website', 'website_view'],
        ['Database', 'database_view'],
        ['Cronjob', 'cronjob_view'],
        ['AppInstalled', 'app_view'],
        ['File', 'host_file_view'],
    ].map(([name, permission]) => [name, useCan(permission)]),
) as Record<string, ReturnType<typeof useCan>>;

const checkPermission = (item: string) => {
    return quickJumpPermissionMap[item]?.value ?? true;
};

const searchInfo = reactive({
    ioOption: 'all',
    netOption: 'all',
});

const memoContent = ref('');
const memoEditContent = ref('');
const memoEditing = ref(false);
const memoSaving = ref(false);
const memoCarouselSetting = ref();
const simpleNodeCarouselSetting = ref();
const carouselSettingReady = ref(false);

const showMemoCarousel = computed(() => memoCarouselSetting.value === 'Enable');
const carouselItemCount = computed(() => {
    let count = 1;
    if (showMemoCarousel.value) count += 1;
    if (showSimpleNode()) count += 1;
    return count;
});

const baseInfo = ref<Dashboard.BaseInfo>({
    hostname: '',
    os: '',
    platform: '',
    platformFamily: '',
    platformVersion: '',
    prettyDistro: '',
    kernelArch: '',
    kernelVersion: '',
    virtualizationSystem: '',
    ipV4Addr: '',
    httpProxy: '',

    cpuCores: 0,
    cpuLogicalCores: 0,
    cpuModelName: '',
    cpuMhz: 0,
    currentInfo: null,

    quickJump: [],
});
const currentInfo = ref<Dashboard.CurrentInfo>({
    uptime: 0,
    timeSinceUptime: '',
    runningTime: {
        days: 0,
        hours: 0,
        minutes: 0,
        seconds: 0,
    },
    procs: 0,

    load1: 0,
    load5: 0,
    load15: 0,
    loadUsagePercent: 0,

    cpuPercent: [] as Array<number>,
    cpuUsedPercent: 0,
    cpuUsed: 0,
    cpuTotal: 0,
    cpuDetailedPercent: [] as Array<number>,

    memoryTotal: 0,
    memoryAvailable: 0,
    memoryUsed: 0,
    memoryFree: 0,
    memoryShard: 0,
    memoryCache: 0,
    memoryUsedPercent: 0,
    swapMemoryTotal: 0,
    swapMemoryAvailable: 0,
    swapMemoryUsed: 0,
    swapMemoryUsedPercent: 0,

    ioReadBytes: 0,
    ioWriteBytes: 0,
    ioCount: 0,
    ioReadTime: 0,
    ioWriteTime: 0,

    diskData: [],
    gpuData: [],
    xpuData: [],

    netBytesSent: 0,
    netBytesRecv: 0,

    topCPUItems: [],
    topMemItems: [],

    shotTime: new Date(),
});
const currentChartInfo = reactive({
    ioReadBytes: 0,
    ioWriteBytes: 0,
    ioCount: 0,
    ioTime: 0,

    netBytesSent: 0,
    netBytesRecv: 0,
});
const skipNextCurrentInfoDelta = ref(false);

const chartsOption = ref({
    ioChart: loadIOChartOption(),
    networkChart: loadNetworkChartOption(),
});

const updateCurrentInfo = (data: Dashboard.CurrentInfo) => {
    currentInfo.value = {
        ...data,
        topCPUItems: currentInfo.value.topCPUItems || [],
        topMemItems: currentInfo.value.topMemItems || [],
    };
};

const changeOption = async () => {
    isInit.value = true;
    loadData();
};

const applyDefaultNetOption = () => {
    if (!netOptions.value || netOptions.value.length === 0) return;
    const defaultNet = defaultNetwork.value || netOptions.value[0];
    if (defaultNet && searchInfo.netOption !== defaultNet) {
        searchInfo.netOption = defaultNet;
    }
};

const onLoadAgentSettingInfo = async () => {
    await getAgentSettingInfo().then((res) => {
        defaultIO.value = res.data.defaultIO;
        defaultNetwork.value = res.data.defaultNetwork;
    });
};

const onLoadNetworkOptions = async (force?: boolean) => {
    const cache = force ? null : getDashboardCache('netOptions');
    if (cache !== null) {
        netOptions.value = cache;
        netOptionsFromCache.value = true;
        applyDefaultNetOption();
        return;
    }
    const res = await getNetworkOptions();
    netOptions.value = res.data;
    netOptionsFromCache.value = false;
    setDashboardCache('netOptions', res.data, DASHBOARD_CACHE_TTL.netOptions);
    applyDefaultNetOption();
};

const onLoadSimpleNode = async () => {
    if (!isAdmin.value) {
        simpleNodes.value = [];
        return;
    }
    const res = await listAllSimpleNodes();
    simpleNodes.value = res.data || [];
};

const applyDefaultIOOption = async () => {
    if (!ioOptions.value || ioOptions.value.length === 0) return;
    const defaultIOOption = defaultIO.value || ioOptions.value[0];
    if (defaultIOOption && searchInfo.ioOption !== defaultIOOption) {
        searchInfo.ioOption = defaultIOOption;
    }
};

const onLoadIOOptions = async (force?: boolean) => {
    const cache = force ? null : getDashboardCache('ioOptions');
    if (cache !== null) {
        ioOptions.value = cache;
        ioOptionsFromCache.value = true;
        applyDefaultIOOption();
        return;
    }
    const res = await getIOOptions();
    ioOptions.value = res.data;
    ioOptionsFromCache.value = false;
    setDashboardCache('ioOptions', ioOptions.value, DASHBOARD_CACHE_TTL.ioOptions);
    applyDefaultIOOption();
};

const onLoadBaseInfo = async (isInit: boolean, range: string) => {
    let resetChartData = false;
    if (range === 'all' || range === 'io') {
        ioReadBytes.value = [];
        ioWriteBytes.value = [];
        timeIODatas.value = [];
        resetChartData = true;
    }
    if (range === 'all' || range === 'network') {
        netBytesSents.value = [];
        netBytesRecvs.value = [];
        timeNetDatas.value = [];
        resetChartData = true;
    }
    if (resetChartData) {
        loadData();
    }
    const res = await loadBaseInfo(searchInfo.ioOption, searchInfo.netOption);
    baseInfo.value = res.data;
    updateCurrentInfo(baseInfo.value.currentInfo);
    skipNextCurrentInfoDelta.value = true;
    onLoadCurrentInfo();
    isStatusInit.value = false;
    statusRef.value?.acceptParams(currentInfo.value, baseInfo.value);
    appRef.value?.acceptParams();
    if (isInit) {
        clearTimer();
        timer = setInterval(async () => {
            try {
                if (!isCurrentActive.value) {
                    throw new Error('jump out');
                }
                if (isActive.value && !isOnRestart.value) {
                    await onLoadCurrentInfo();
                    await onLoadSimpleNode();
                }
            } catch {
                clearTimer();
            }
        }, 3000);
    }
};

const quickJump = (item: any) => {
    if (item.name === 'File') {
        return routerToFileWithPath(item.detail);
    }
    return routerToPath(item.router);
};

const showSimpleNode = () => {
    return simpleNodeCarouselSetting.value === 'Enable' && isXpackOrEE.value && simpleNodes.value?.length !== 0;
};

const toggleSensitiveInfo = () => {
    showSensitiveInfo.value = !showSensitiveInfo.value;
};

const refreshDashboard = async () => {
    clearDashboardCache();
    onLoadBaseInfo(false, '');
    hasRefreshedOptionsOnHover.value = false;
    await Promise.allSettled([onLoadNetworkOptions(true), onLoadIOOptions(true), loadSettingInfo()]);
    MsgSuccess(i18n.global.t('commons.msg.refreshSuccess'));
};

const jumpPanel = (row: any) => {
    let entrance = row.securityEntrance.startsWith('/') ? row.securityEntrance.slice(1) : row.securityEntrance;
    entrance = entrance ? '/' + entrance : '';
    let addr = row.addr.endsWith('/') ? row.addr.slice(0, -1) : row.addr;
    window.open(addr + entrance, '_blank', 'noopener,noreferrer');
};

const onLoadCurrentInfo = async () => {
    const res = await loadCurrentInfo(searchInfo.ioOption, searchInfo.netOption);
    if (skipNextCurrentInfoDelta.value) {
        skipNextCurrentInfoDelta.value = false;
        currentChartInfo.netBytesSent = 0;
        currentChartInfo.netBytesRecv = 0;
        currentChartInfo.ioReadBytes = 0;
        currentChartInfo.ioWriteBytes = 0;
        currentChartInfo.ioCount = 0;
        currentChartInfo.ioTime = 0;
        updateCurrentInfo(res.data);
        statusRef.value?.acceptParams(currentInfo.value, baseInfo.value);
        return;
    }

    currentInfo.value.timeSinceUptime = res.data.timeSinceUptime;
    currentInfo.value.runningTime = res.data.runningTime;

    let timeInterval = Number(res.data.uptime - currentInfo.value.uptime) || 3;
    currentChartInfo.netBytesSent =
        res.data.netBytesSent - currentInfo.value.netBytesSent > 0
            ? Number(((res.data.netBytesSent - currentInfo.value.netBytesSent) / 1024 / timeInterval).toFixed(2))
            : 0;
    netBytesSents.value.push(currentChartInfo.netBytesSent);
    if (netBytesSents.value.length > 20) {
        netBytesSents.value.splice(0, 1);
    }

    currentChartInfo.netBytesRecv =
        res.data.netBytesRecv - currentInfo.value.netBytesRecv > 0
            ? Number(((res.data.netBytesRecv - currentInfo.value.netBytesRecv) / 1024 / timeInterval).toFixed(2))
            : 0;
    netBytesRecvs.value.push(currentChartInfo.netBytesRecv);
    if (netBytesRecvs.value.length > 20) {
        netBytesRecvs.value.splice(0, 1);
    }

    currentChartInfo.ioReadBytes =
        res.data.ioReadBytes - currentInfo.value.ioReadBytes > 0
            ? Number(((res.data.ioReadBytes - currentInfo.value.ioReadBytes) / 1024 / 1024 / timeInterval).toFixed(2))
            : 0;
    ioReadBytes.value.push(currentChartInfo.ioReadBytes);
    if (ioReadBytes.value.length > 20) {
        ioReadBytes.value.splice(0, 1);
    }

    currentChartInfo.ioWriteBytes =
        res.data.ioWriteBytes - currentInfo.value.ioWriteBytes > 0
            ? Number(((res.data.ioWriteBytes - currentInfo.value.ioWriteBytes) / 1024 / 1024 / timeInterval).toFixed(2))
            : 0;
    ioWriteBytes.value.push(currentChartInfo.ioWriteBytes);
    if (ioWriteBytes.value.length > 20) {
        ioWriteBytes.value.splice(0, 1);
    }
    currentChartInfo.ioCount = Math.round(Number((res.data.ioCount - currentInfo.value.ioCount) / timeInterval));
    let ioReadTime = res.data.ioReadTime - currentInfo.value.ioReadTime;
    let ioWriteTime = res.data.ioWriteTime - currentInfo.value.ioWriteTime;
    let ioChoose = ioReadTime > ioWriteTime ? ioReadTime : ioWriteTime;
    currentChartInfo.ioTime = Math.round(Number(ioChoose / timeInterval));

    timeIODatas.value.push(dateFormatForSecond(res.data.shotTime));
    if (timeIODatas.value.length > 20) {
        timeIODatas.value.splice(0, 1);
    }
    timeNetDatas.value.push(dateFormatForSecond(res.data.shotTime));
    if (timeNetDatas.value.length > 20) {
        timeNetDatas.value.splice(0, 1);
    }
    loadData();
    updateCurrentInfo(res.data);
    statusRef.value?.acceptParams(currentInfo.value, baseInfo.value);
};

const handleCopy = () => {
    let content =
        i18n.global.t('home.hostname') +
        ': ' +
        baseInfo.value.hostname +
        '\n' +
        i18n.global.t('home.platformVersion') +
        ': ' +
        (baseInfo.value.prettyDistro
          