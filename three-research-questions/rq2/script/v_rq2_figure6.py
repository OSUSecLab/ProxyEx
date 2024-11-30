import numpy as np
import matplotlib.pyplot as plt
import json


# Project and file paths
project_path = "../data/"
upgrade_freq_list_path = project_path + "v_rq2_figure6.txt"

# Load the upgrade frequency list
with open(upgrade_freq_list_path, 'r') as file:
    upgrade_freq_list = json.load(file)
print("Length of upgrade freq list ====> ", len(upgrade_freq_list))

# Sort the data
total_upgrade_freq_list = np.sort(upgrade_freq_list)

# Maximum upgrade frequency
max_upgrade_freq = max(total_upgrade_freq_list)
print("MAX UPGRADE FREQUENCY ====> ", max_upgrade_freq)

# Designing the plot
plt.rcParams['axes.unicode_minus'] = False
plt.rcParams.update({'font.size': 30})
fig, ax1 = plt.subplots(figsize=(16, 9))

# Calculate counts and unique values
unique_values, counts = np.unique(total_upgrade_freq_list, return_counts=True)

# Plotting the bar chart
ax1.bar(unique_values, counts, color='r', alpha=0.3)
ax1.set_xlabel('Upgrade Frequency', fontdict={'fontsize': 30})
ax1.set_xlim(-1, max_upgrade_freq+5)
ax1.set_ylabel('Proxy Count', fontdict={'fontsize': 30})
ax1.set_yscale('log')
ax1.set_ylim(0, max(counts)+1250000)

# Calculate CDF
pdf = counts / np.sum(counts)
cdf_values = np.cumsum(pdf)

# Prepend a data point at (0, 0)
unique_values = np.insert(unique_values, 0, 0)
cdf_values = np.insert(cdf_values, 0, 0)

# Plotting the CDF
ax2 = ax1.twinx()
ax2.plot(unique_values, cdf_values, color='b')
ax2.set_ylabel('CDF', fontdict={'fontsize': 30})
ax2.set_yticklabels([0, 0.2, 0.4, 0.6, 0.8, 1.0])
ax2.set_ylim(0, 1.05)

# Save the figure
plt.savefig(project_path + "v_rq2_figure6.pdf",
            facecolor='white', bbox_inches='tight')

# Show the plot
plt.show()

# End of Program
