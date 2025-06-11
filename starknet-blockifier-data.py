import pandas as pd
import numpy as np
from pandas import DataFrame
from argparse import ArgumentParser

parser = ArgumentParser()
parser.add_argument("blockifier_data")
parser.add_argument("replay_data")
args = parser.parse_args()

df_blockifier: pd.DataFrame = pd.read_csv(args.blockifier_data)
df_blockifier.rename(
    {"chunk_execution_duration_ms": "duration_ms"},
    axis=1,
    inplace=True,
)
is_header = df_blockifier["timestamp"] == "timestamp"
df_blockifier = df_blockifier[~is_header]  # type: ignore
df_blockifier["timestamp"] = pd.to_datetime(
    df_blockifier["timestamp"], format="%Y-%m-%d %H:%M:%S,%f"
)
df_blockifier["duration_ms"] = pd.to_numeric(df_blockifier["duration_ms"])

# print(df_blockifier.info())
# --------------------------------
#  #   Column       Dtype
# --------------------------------
#  0   duration_ms  object
#  1   timestamp    datetime64[ns]

df_replay = pd.read_csv(
    args.replay_data, parse_dates=[1, 2], date_format="%Y-%m-%d %H:%M:%S,%f"
)
df_replay.set_index("Run #", inplace=True, drop=False)
df_replay.rename(
    {
        "Run #": "run-number",
        "Replay Start": "timestamp-start",
        "Replay End": "timestamp-end",
        "Concurrency": "with-concurrency",
        "Cairo Native": "with-cairo-native",
        "Optimization Level": "cairo-native-opt",
    },
    axis=1,
    inplace=True,
)


# print(df_replay.info())
# -------------------------------------
# #   Column             Dtype
# -------------------------------------
# 0   run-number         float64
# 1   timestamp-start    datetime64[ns]
# 2   timestamp-end      datetime64[ns]
# 3   with-concurrency   bool
# 4   with-cairo-native  bool
# 5   cairo-native-opt   object


def run_by_timestamp(timestamp: np.datetime64, df_replay: DataFrame):
    for run_number, run_data in df_replay.iterrows():
        if (
            timestamp >= run_data["timestamp-start"]
            and timestamp <= run_data["timestamp-end"]
        ):
            return run_number

    return None


df_blockifier["run-number"] = df_blockifier["timestamp"].apply(
    lambda timestamp: run_by_timestamp(timestamp, df_replay)
)
df_blockifier.dropna(inplace=True)

df_runs = df_blockifier.groupby("run-number").aggregate(
    total_duration_ms=("duration_ms", "sum"),
    mean_duration_ms=("duration_ms", "mean"),
)

print(df_runs)
