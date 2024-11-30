import numpy as np
import matplotlib.pyplot as plt
import json


# Project and file paths
project_path = "../data/"
lifespan_file = project_path + "iv_rq1_figure5.txt"

# Open the lifespan file in read mode
lifespan_list = []
with open(lifespan_file, 'r') as file:
    lifespan_list = json.load(file)
print("Length of timespan list ====> ", len(lifespan_list))

# Set the maximum count as 2000
max_list = 2000

# Sort the list
lifespan_file = np.sort(lifespan_list)

# Calculate counts and unique values
ts_unique_values, ts_counts = np.unique(lifespan_file, return_counts=True)
max_ts_count = max(ts_counts)

# Designing the plot
plt.rcParams['axes.unicode_minus'] = False
plt.rcParams.update({'font.size': 36})
fig, ax1 = plt.subplots(figsize=(16, 9))
plt.xlim(-20, max_list+20)

# Plotting the bar chart
ax1.bar(ts_unique_values, ts_counts,  color='r', alpha=0.3, width=1)
ax1.ticklabel_format(style='plain')
ax1.set_xlabel('Day', fontdict={'fontsize': 36})
ax1.set_xticks(np.arange(0, max_list+1, 400))
ax1.set_ylabel('Proxy Count', fontdict={'fontsize': 36})
ax1.set_yscale('log')
ax1.set_ylim(0, max_ts_count)

# Computing the CDF
ts_pdf = ts_counts / np.sum(ts_counts)
ts_cdf = np.cumsum(ts_pdf)

# Prepend a data point at (0, 0)
ts_cdf = np.insert(ts_cdf, 0, 0)
ts_unique_values = np.insert(ts_unique_values, 0, 0)

# Plotting the CDF
ax2 = ax1.twinx()
ax2.plot(ts_unique_values, ts_cdf, color="b")
ax2.set_ylabel('CDF', fontdict={'fontsize': 36})
ax2.set_yticklabels([0, 0.2, 0.4, 0.6, 0.8, 1.0])
ax2.set_ylim(0, 1.05)

# Save the plot
plt.savefig(project_path + "iv_rq1_figure5.pdf",
            facecolor='white', bbox_inches='tight')

# Show the plot
plt.show()

# End of Program
