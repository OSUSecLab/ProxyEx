import numpy as np
import matplotlib.pyplot as plt
import pandas as pd


# Project and file paths
project_path = "../data/"
bytecode_duplicate_file = project_path + "iv_rq1_figure3.csv"

# Load the data
df = pd.read_csv(bytecode_duplicate_file,
                 usecols=[0, 1], names=['file', 'duplicate_count'])

# Extracting the data as a numpy array
bytecode_dupl_count_list = df['duplicate_count'].values
print("Bytecode dupl count length =====>", len(bytecode_dupl_count_list))

# Sort the data
bytecode_dupl_count_list = np.sort(bytecode_dupl_count_list)

# Set a threshold of 20
threshold = 20
bytecode_dupl_count_list = bytecode_dupl_count_list[bytecode_dupl_count_list <= threshold]

# Designing the plot
plt.rcParams['axes.unicode_minus'] = False
plt.rcParams.update({'font.size': 36})
fig, ax1 = plt.subplots(figsize=(16, 9))
plt.xlim(-0.25, 20.25)

# Calculate counts and unique values
unique_values, counts = np.unique(bytecode_dupl_count_list, return_counts=True)

# Plotting the bar chart
ax1.bar(unique_values, counts,  color='r', alpha=0.3)
ax1.ticklabel_format(style='plain')
ax1.set_xlabel('Count', fontdict={'fontsize': 36})
ax1.set_ylabel('Proxy Count', fontdict={'fontsize': 36})
ax1.set_yticks(np.arange(0, 4100, 800))
ax1.set_ylim(0, max(counts) * 1.05)

# Computing the CDF
pdf = counts / np.sum(counts)
cdf = np.cumsum(pdf)

# Prepend a data point at (0, 0)
cdf = np.insert(cdf, 0, 0)
unique_values = np.insert(unique_values, 0, 0)

# Plotting the CDF
ax2 = ax1.twinx()
ax2.plot(unique_values, cdf, label="CDF", color="b")
ax2.set_ylabel('CDF', fontdict={'fontsize': 36})
ax2.set_yticklabels([0, 0.2, 0.4, 0.6, 0.8, 1.0])
ax2.set_ylim(0, 1.05)

# Save the plot
plt.savefig(project_path + "iv_rq1_figure3.pdf",
            facecolor='white', bbox_inches='tight')

# Show the plot
plt.show()

# End of Program
