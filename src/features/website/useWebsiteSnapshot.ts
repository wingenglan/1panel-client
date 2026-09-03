import { useQuery } from "@tanstack/react-query";
import { api } from "../../lib/api";

/** 网站模块四个子页共用的受控站点快照查询，键与刷新入口统一。
 *  refetchInterval 由「不刷新」开关控制（Web 面板默认每 15 秒自动刷新）。 */
export function useWebsiteSnapshot(serverId: string, options: { refetchInterval?: number | false } = {}) {
  return useQuery({
    queryKey: ["websites", serverId],
    queryFn: () => api.websites(serverId),
    enabled: Boolean(serverId),
    refetchInterval: options.refetchInterval,
  });
}
